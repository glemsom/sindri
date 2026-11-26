use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sindri_core::config::{Config, ConfigError, config_path};
use sindri_core::provider::Route;
use sindri_core::wire::Wire;
use tempfile::TempDir;

fn environment(vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    vars.iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn config_home(text: &str) -> TempDir {
    let home = tempfile::tempdir().expect("a temp dir");
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    home
}

fn resolved(home: &Path, vars: &[(&str, &str)], flags: Config) -> Result<Config, ConfigError> {
    let mut environment = environment(vars);
    environment.insert(String::from("XDG_CONFIG_HOME"), home.display().to_string());
    Config::resolve(&environment, flags)
}

fn resolved_in(text: &str, vars: &[(&str, &str)], flags: Config) -> Config {
    let home = config_home(text);
    resolved(home.path(), vars, flags).expect("the config loads")
}

const EXAMPLES: [(&str, &str); 21] = [
    ("provider", "\"openai\""),
    ("model", "\"claude-sonnet-4-5\""),
    ("route", "\"responses\""),
    ("base_url", "\"https://gateway.test/v1\""),
    ("api_key", "\"sk-not-a-real-key\""),
    ("responses_continuation", "true"),
    ("responses_store", "true"),
    ("responses_continuation_verified", "true"),
    ("thinking", "true"),
    ("extra_writable_paths", "[\"/srv/data\"]"),
    ("extra_headers", "[\"X-Gateway-Session: {session}\"]"),
    ("context_window", "200000"),
    ("output_headroom", "4096"),
    ("tool_result_headroom", "4096"),
    ("max_turns", "250"),
    ("timeout", "120"),
    ("command_budget_ms", "3600000"),
    ("output_budget_bytes", "8388608"),
    ("status", "true"),
    ("face", "false"),
    ("ssh_agent", "false"),
];

const SPOKEN: [(&str, &str, &str); 21] = [
    ("provider", "openai", "\"openai\""),
    ("model", "claude-sonnet-4-5", "\"claude-sonnet-4-5\""),
    ("route", "responses", "\"responses\""),
    (
        "base_url",
        "https://gateway.test/",
        "\"https://gateway.test/\"",
    ),
    ("api_key", "sk-not-a-real-key", "\"sk-not-a-real-key\""),
    ("responses_continuation", "true", "true"),
    ("responses_store", "true", "true"),
    ("responses_continuation_verified", "true", "true"),
    ("thinking", "true", "true"),
    (
        "extra_writable_paths",
        "/srv/data:/srv/cache",
        "[\"/srv/data\", \"/srv/cache\"]",
    ),
    (
        "extra_headers",
        "X-Gateway-Session: {session}\nX-Trace: on",
        "[\"X-Gateway-Session: {session}\", \"X-Trace: on\"]",
    ),
    ("context_window", "200000", "200000"),
    ("output_headroom", "4096", "4096"),
    ("tool_result_headroom", "4096", "4096"),
    ("max_turns", "250", "250"),
    ("timeout", "120", "120"),
    ("command_budget_ms", "3600000", "3600000"),
    ("output_budget_bytes", "8388608", "8388608"),
    ("status", "true", "true"),
    ("face", "false", "false"),
    ("ssh_agent", "false", "false"),
];

fn examples_document() -> String {
    EXAMPLES
        .iter()
        .map(|(key, example)| format!("{key} = {example}\n"))
        .collect()
}

fn worked(key: &str) -> &'static (&'static str, &'static str, &'static str) {
    SPOKEN
        .iter()
        .find(|(name, _, _)| *name == key)
        .unwrap_or_else(|| panic!("the worked examples name `{key}`"))
}

fn file_read(text: &str) -> Config {
    let home = config_home(text);
    Config::load_from(&home.path().join("sindri/config.toml")).expect("the file reads")
}

fn the_key_alone(key: &str, file: &str) -> Config {
    file_read(&format!("{key} = {file}\n"))
}

#[test]
fn an_unknown_key_names_the_key_and_the_file() {
    let home = config_home("modle = \"claude-sonnet-4-5\"\n");
    let path = home.path().join("sindri").join("config.toml");

    let error =
        resolved(home.path(), &[], Config::default()).expect_err("an unknown key is an error");

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

    let error = resolved(home.path(), &[], Config::default()).expect_err("a bad value is an error");

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
        Config::SCHEMA.len(),
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
    assert_eq!(config.context_window, Some(200_000));
    assert_eq!(config.max_turns, Some(250));
    assert_eq!(config.timeout, Some(120));
    assert_eq!(config.command_budget_ms, Some(3_600_000));
    assert_eq!(config.output_budget_bytes, Some(8_388_608));
    assert_eq!(config.status, Some(true));
    assert!(
        !config.draws_face(),
        "a file that turns the face off is read as the frame's own answer"
    );
    assert_eq!(config.ssh_agent, Some(false));
    assert_eq!(config.thinking, Some(true));
    assert_eq!(
        config.extra_headers,
        Some(vec![String::from("X-Gateway-Session: {session}")])
    );
}

#[test]
fn the_table_names_every_key_of_the_schema_in_the_order_the_screen_reads_them() {
    let order = [
        "provider",
        "model",
        "route",
        "base_url",
        "api_key",
        "responses_continuation",
        "responses_store",
        "responses_continuation_verified",
        "thinking",
        "extra_writable_paths",
        "extra_headers",
        "context_window",
        "output_headroom",
        "tool_result_headroom",
        "max_turns",
        "timeout",
        "command_budget_ms",
        "output_budget_bytes",
        "status",
        "face",
        "ssh_agent",
    ];

    let named: Vec<&str> = Config::SCHEMA.iter().map(|field| field.name).collect();

    assert_eq!(
        named, order,
        "one table names every key of the schema, in the order the settings screen reads them"
    );

    let switches: Vec<&str> = Config::SCHEMA
        .iter()
        .filter(|field| field.switch())
        .map(|field| field.name)
        .collect();

    assert_eq!(
        switches,
        vec![
            "responses_continuation",
            "responses_store",
            "responses_continuation_verified",
            "thinking",
            "status",
            "face",
            "ssh_agent"
        ],
        "a key the table calls a switch is one the screen's space flips, and no other key is"
    );
}

#[test]
fn the_face_is_drawn_unless_the_config_says_otherwise_and_its_key_sits_beside_status() {
    let keys: Vec<&str> = Config::SCHEMA.iter().map(|field| field.name).collect();
    let at = keys
        .iter()
        .position(|key| *key == "face")
        .expect("the schema names the face");

    assert_eq!(
        (keys.get(at.wrapping_sub(1)), keys.get(at + 1)),
        (Some(&"status"), Some(&"ssh_agent")),
        "the face is spelled beside the status key and the agent's, the two switches it keeps \
         company with: {keys:?}"
    );
    assert!(
        Config::field("face").is_some_and(|field| field.switch()),
        "a key that holds on or off is one the settings screen's space flips"
    );
    assert!(
        Config::default().draws_face(),
        "a config nothing has spoken for draws the face, so a launch that says nothing is a \
         frame like every other"
    );
}

#[test]
fn an_exported_false_turns_the_face_off_for_one_launch_and_no_longer() {
    let home = config_home("");
    let exported = resolved(home.path(), &[("SINDRI_FACE", "false")], Config::default())
        .expect("an exported false is a value the schema holds");
    assert!(
        !exported.draws_face(),
        "the environment is the door a face is turned off at for one launch"
    );

    let after = resolved(home.path(), &[], Config::default()).expect("the config loads");
    assert!(
        after.draws_face(),
        "and the next launch is the file's own answer, which said nothing"
    );
}

#[test]
fn an_exported_true_turns_the_face_back_on_over_a_file_that_turned_it_off() {
    let home = config_home("face = false\n");
    let exported = resolved(home.path(), &[("SINDRI_FACE", "true")], Config::default())
        .expect("the config loads");

    assert!(
        exported.draws_face(),
        "the environment is read above the file, as every other flag is"
    );
}

#[test]
fn a_context_window_in_the_file_is_read_as_tokens_beside_the_turn_cap() {
    let keys: Vec<&str> = Config::SCHEMA.iter().map(|field| field.name).collect();
    let at = keys
        .iter()
        .position(|key| *key == "context_window")
        .expect("the schema names the window");

    assert_eq!(
        keys.get(at + 3),
        Some(&"max_turns"),
        "the settings modal walks the schema's order, so the window is one of the run dials \
         and its headroom sits before the cap: {keys:?}"
    );

    let home = config_home("context_window = 200000\n");
    let config = resolved(home.path(), &[], Config::default()).expect("the config loads");

    assert_eq!(
        config.context_window,
        Some(200_000),
        "the window is the config's to own: no catalogue holds one and no wire reports one"
    );
}

#[test]
fn a_context_window_of_nothing_is_refused_naming_the_key() {
    let home = config_home("context_window = 0\n");
    let path = home.path().join("sindri").join("config.toml");

    let error = resolved(home.path(), &[], Config::default())
        .expect_err("a window of no tokens is a share nothing can be taken of");

    assert_eq!(error.exit_code(), 2);
    let message = error.to_string();
    assert!(
        message.contains("at least one token") && message.contains(&path.display().to_string()),
        "the refusal says the smallest window that would do and the file it was written in, \
         and it says: {message}"
    );
}

#[test]
fn an_unset_context_window_is_no_window_at_all() {
    let home = config_home("");

    let config = resolved(home.path(), &[], Config::default()).expect("the config loads");

    assert_eq!(
        config.context_window, None,
        "leaving the window unset is legal and is the default: no window, no percentage, \
         and nothing guessed"
    );
}

#[test]
fn the_context_window_layers_file_then_environment_then_flag() {
    let home = config_home("context_window = 200000\n");
    let flag = Config {
        context_window: Some(50_000),
        ..Config::default()
    };

    let from_file = resolved(home.path(), &[], Config::default()).expect("the config loads");
    let from_environment = resolved(
        home.path(),
        &[("SINDRI_CONTEXT_WINDOW", "100000")],
        Config::default(),
    )
    .expect("the config loads");
    let from_flag = resolved(home.path(), &[("SINDRI_CONTEXT_WINDOW", "100000")], flag)
        .expect("the config loads");

    assert_eq!(
        from_file.context_window,
        Some(200_000),
        "the file speaks when nothing above it does"
    );
    assert_eq!(
        from_environment.context_window,
        Some(100_000),
        "an exported window wins over the file"
    );
    assert_eq!(
        from_flag.context_window,
        Some(50_000),
        "the flag is read above both, and replaces rather than adds"
    );
}

#[test]
fn an_exported_nothing_leaves_the_files_window_speaking() {
    let home = config_home("context_window = 200000\n");

    let config = resolved(
        home.path(),
        &[("SINDRI_CONTEXT_WINDOW", "")],
        Config::default(),
    )
    .expect("the config loads");

    assert_eq!(
        config.context_window,
        Some(200_000),
        "an exported nothing is silence, so the key the file speaks is still the key in force"
    );
}

#[test]
fn a_context_window_of_nothing_in_the_environment_names_the_variable() {
    let home = config_home("");

    let error = resolved(
        home.path(),
        &[("SINDRI_CONTEXT_WINDOW", "0")],
        Config::default(),
    )
    .expect_err("a window of no tokens is a share nothing can be taken of");

    assert_eq!(error.exit_code(), 2);
    let message = error.to_string();
    assert!(
        message.contains("SINDRI_CONTEXT_WINDOW") && message.contains("at least one token"),
        "the refusal names the variable a person has to fix, and it says: {message}"
    );
}

#[test]
fn debug_never_prints_the_api_key() {
    let home = config_home("api_key = \"sk-not-a-real-key\"\n");

    let config = resolved(home.path(), &[], Config::default()).expect("the config loads");

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

struct Standing {
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
        Config::default(),
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
        Config {
            model: Some(String::from("from-the-flag")),
            ..Config::default()
        },
    )
    .expect("the config loads");

    assert_eq!(config.model.as_deref(), Some("from-the-flag"));
}

#[test]
fn the_wire_own_variable_is_where_a_launch_takes_its_key_from() {
    let home = config_home("provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n");

    let config = resolved(
        home.path(),
        &[("ANTHROPIC_API_KEY", "sk-ant-the-wires-own")],
        Config::default(),
    )
    .expect("the config loads");

    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-ant-the-wires-own"),
        "no flag, no SINDRI_API_KEY and no key in the file: the variable the wire names supplies it"
    );
}

#[test]
fn a_config_whose_key_only_the_wire_own_variable_holds_is_ready() {
    let config = resolved_in(
        "provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\n",
        &[("ANTHROPIC_API_KEY", "sk-ant-the-wires-own")],
        Config::default(),
    );

    assert!(
        config.is_ready(),
        "a wire, a model and the key its own variable holds are a run that starts: {:?}",
        config.missing()
    );
    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-ant-the-wires-own"),
        "and it is that variable's value the config holds, not another's"
    );
}

#[test]
fn the_wire_the_layering_names_is_the_one_whose_variable_is_read() {
    let home = config_home("provider = \"openai\"\nmodel = \"gpt-5\"\n");

    let config = resolved(
        home.path(),
        &[
            ("OPENAI_API_KEY", "sk-the-openai-one"),
            ("ANTHROPIC_API_KEY", "sk-the-anthropic-one"),
        ],
        Config::default(),
    )
    .expect("the config loads");

    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-the-openai-one"),
        "the file names openai, so the variable an openai run reads is the one read: another \
         wire's variable is not a credential for this run"
    );
}

#[test]
fn a_wire_that_signs_in_names_no_variable_to_read() {
    let home = config_home("provider = \"copilot\"\nmodel = \"gpt-5\"\n");

    let config = resolved(
        home.path(),
        &[("ANTHROPIC_API_KEY", "sk-the-anthropic-one")],
        Config::default(),
    )
    .expect("the config loads");

    assert_eq!(
        config.api_key, None,
        "copilot signs in rather than holding a key, so it names no variable and reads none"
    );
}

#[test]
fn the_flag_supplies_the_key_and_wins_over_the_exported_one() {
    let config = resolved_in(
        "",
        &[
            ("SINDRI_PROVIDER", "anthropic"),
            ("SINDRI_MODEL", "claude-sonnet-4-5"),
            ("ANTHROPIC_API_KEY", "sk-ant-the-exported-one"),
        ],
        Config {
            api_key: Some(String::from("sk-ant-the-flag")),
            ..Config::default()
        },
    );

    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-ant-the-flag"),
        "a flag is read above the environment, which is read above the file"
    );
}

#[test]
fn sindris_own_variable_wins_over_the_wires() {
    let config = resolved_in(
        "",
        &[
            ("SINDRI_PROVIDER", "anthropic"),
            ("SINDRI_MODEL", "claude-sonnet-4-5"),
            ("SINDRI_API_KEY", "sk-ant-named-explicitly"),
            ("ANTHROPIC_API_KEY", "sk-ant-the-fallback"),
        ],
        Config::default(),
    );

    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-ant-named-explicitly"),
        "the variable that names Sindri is more specific than the wire's own"
    );
}

#[test]
fn an_exported_key_wins_over_a_key_in_the_file() {
    let config = resolved_in(
        "provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\napi_key = \"sk-ant-in-the-file\"\n",
        &[("ANTHROPIC_API_KEY", "sk-ant-exported")],
        Config::default(),
    );

    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-ant-exported"),
        "an exported key wins, which is the whole point of reading the environment above the file"
    );
}

#[test]
fn a_key_in_the_file_is_the_last_word_when_no_variable_holds_one() {
    let config = resolved_in(
        "provider = \"anthropic\"\nmodel = \"claude-sonnet-4-5\"\napi_key = \"sk-ant-in-the-file\"\n",
        &[],
        Config::default(),
    );

    assert_eq!(
        config.api_key.as_deref(),
        Some("sk-ant-in-the-file"),
        "nothing above the file spoke and the wire's variable is empty, so the key a person \
         wrote is the one the run holds"
    );
}

#[test]
fn the_route_a_run_takes_layers_file_then_environment_then_flag() {
    let home = config_home("route = \"responses\"\n");
    let flag = Config {
        route: Some(Route::Chat),
        ..Config::default()
    };

    let from_file = resolved(home.path(), &[], Config::default()).expect("the config loads");
    let from_environment = resolved(home.path(), &[("SINDRI_ROUTE", "chat")], Config::default())
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

    let error = resolved(home.path(), &[], Config::default()).expect_err("a bad route");

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
        Config::default(),
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
        Config {
            extra_writable_paths: Some(vec![PathBuf::from("/from-the-flag")]),
            ..Config::default()
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

    let config = resolved(home.path(), &[("SINDRI_MODEL", "")], Config::default())
        .expect("the config loads");

    assert_eq!(config.model.as_deref(), Some("from-the-file"));
}

#[test]
fn the_environment_reads_a_path_list_the_way_the_shell_does() {
    let home = config_home("extra_writable_paths = [\"/from-the-file\"]\n");

    let config = resolved(
        home.path(),
        &[("SINDRI_EXTRA_WRITABLE_PATHS", "/from-the-environment:/also")],
        Config::default(),
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
        Config::default(),
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
        Config::default(),
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

    let error = resolved(home.path(), &[], Config::default())
        .expect_err("a fourth wire cannot be invented");

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

    let config = resolved(home.path(), &[], Config::default()).expect("the config loads");

    assert_eq!(
        config.extra_writable_paths, None,
        "a checked-in config must never grant a writable path"
    );
}

#[test]
fn yolo_unsafe_is_neither_a_key_nor_a_variable() {
    assert!(
        Config::field("yolo_unsafe").is_none(),
        "the escape hatch cannot be persisted, so it is not a key"
    );

    let home = config_home("yolo_unsafe = true\n");
    let error = resolved(home.path(), &[], Config::default())
        .expect_err("the escape hatch cannot be persisted in the file");
    assert!(
        error.to_string().contains("yolo_unsafe"),
        "it says: {error}"
    );

    let home = config_home("");
    let config = resolved(
        home.path(),
        &[("SINDRI_YOLO_UNSAFE", "true")],
        Config::default(),
    )
    .expect("an unknown variable is not an error");

    assert_eq!(
        config,
        Config::default(),
        "no variable can persist an unsafe setting, so this one sets nothing"
    );
}

#[test]
fn two_configs_that_differ_only_in_what_a_frame_reads_decide_the_same_run() {
    let run = Config {
        provider: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        ..Config::default()
    };

    for (key, other) in [
        (
            "status",
            Config {
                status: Some(false),
                ..run.clone()
            },
        ),
        (
            "face",
            Config {
                face: Some(false),
                ..run.clone()
            },
        ),
    ] {
        assert!(
            run.decides_the_same_run(&other),
            "`{key}` is drawn on a frame and asked of no wire, so it decides no run"
        );
    }

    for field in Config::SCHEMA
        .iter()
        .filter(|field| !["status", "face"].contains(&field.name))
    {
        let mut other = run.clone();
        match field.name {
            "provider" => other.provider = Some(Wire::Openai),
            "model" => other.model = Some(String::from("claude-haiku-4-5")),
            "route" => other.route = Some(Route::Responses),
            "base_url" => other.base_url = Some(String::from("https://gateway.test/v1")),
            "api_key" => other.api_key = Some(String::from("sk-another-key")),
            "thinking" => other.thinking = Some(true),
            "responses_continuation" => other.responses_continuation = Some(true),
            "responses_store" => other.responses_store = Some(true),
            "responses_continuation_verified" => other.responses_continuation_verified = Some(true),
            "extra_writable_paths" => other.extra_writable_paths = Some(vec![PathBuf::from("/w")]),
            "extra_headers" => other.extra_headers = Some(vec![String::from("x-one: 1")]),
            "context_window" => other.context_window = Some(200_000),
            "output_headroom" => other.output_headroom = Some(2048),
            "tool_result_headroom" => other.tool_result_headroom = Some(2048),
            "max_turns" => other.max_turns = Some(7),
            "timeout" => other.timeout = Some(30),
            "command_budget_ms" => other.command_budget_ms = Some(1_000),
            "output_budget_bytes" => other.output_budget_bytes = Some(1_024),
            "ssh_agent" => other.ssh_agent = Some(true),
            other => panic!("`{other}` is a key this test does not set"),
        }
        assert!(
            !run.decides_the_same_run(&other),
            "`{}` decides the run a session records, so another value of it is another run",
            field.name
        );
    }
}

#[test]
fn every_row_names_its_own_variable() {
    for field in Config::SCHEMA {
        assert_eq!(
            field.variable,
            format!("SINDRI_{}", field.name.to_uppercase()),
            "a key is spoken by `SINDRI_` and its own name upper-cased, and `{}` is spoken by \
             another",
            field.name
        );
    }

    let mut variables: Vec<&str> = Config::SCHEMA.iter().map(|field| field.variable).collect();
    let rows = variables.len();
    variables.sort_unstable();
    variables.dedup();

    assert_eq!(
        variables.len(),
        rows,
        "no two keys are spoken by one variable: {variables:?}"
    );
}

#[test]
fn every_key_is_read_from_its_own_row() {
    assert_eq!(
        SPOKEN.len(),
        Config::SCHEMA.len(),
        "the worked variables and the key list must name the same keys"
    );

    for field in Config::SCHEMA {
        let (_, variable, file) = worked(field.name);
        let from_the_variable =
            Config::from_environment(&environment(&[(field.variable, variable)]))
                .expect("a worked example is a value its key holds");

        assert_eq!(
            from_the_variable,
            the_key_alone(field.name, file),
            "`{}` is read by the row of `{}`, so it sets that key and no other",
            field.variable,
            field.name
        );
    }
}

#[test]
fn a_variable_holding_a_value_its_key_cannot_hold_is_refused_naming_it() {
    let refused = [
        ("provider", "gemini"),
        ("route", "completions"),
        ("api_key", sindri_core::redact::MASK),
        ("thinking", "maybe"),
        ("extra_headers", "no-colon-here"),
        ("context_window", "0"),
        ("max_turns", "many"),
        ("timeout", "soon"),
        ("command_budget_ms", "later"),
        ("output_budget_bytes", "plenty"),
        ("status", "maybe"),
        ("face", "maybe"),
        ("ssh_agent", "maybe"),
    ];

    for (key, text) in refused {
        let field = Config::field(key).unwrap_or_else(|| panic!("the schema names `{key}`"));

        let error = Config::from_environment(&environment(&[(field.variable, text)]))
            .expect_err(&format!("`{text}` is not a value `{key}` can hold"));

        assert_eq!(
            error.exit_code(),
            2,
            "`{text}` in `{}` is a usage error",
            field.variable
        );
        let message = error.to_string();
        assert!(
            message.contains(field.variable),
            "the refusal names the variable a person has to fix, and it says: {message}"
        );
    }
}

#[test]
fn a_variable_that_says_nothing_yields_to_the_file_for_every_key() {
    let home = config_home(&examples_document());
    let written = resolved(home.path(), &[], Config::default()).expect("the config loads");

    for field in Config::SCHEMA {
        let yielded = resolved(home.path(), &[(field.variable, "")], Config::default())
            .expect("an exported nothing is no value at all");

        assert_eq!(
            yielded, written,
            "`{}` exported empty says nothing, so the file's `{}` is the key in force",
            field.variable, field.name
        );
    }

    let all_silent: Vec<(&str, &str)> = Config::SCHEMA
        .iter()
        .map(|field| (field.variable, ""))
        .collect();
    let silent = resolved(home.path(), &all_silent, Config::default()).expect("the config loads");

    assert_eq!(
        silent, written,
        "every variable exported empty at once leaves the whole file speaking"
    );
}

#[test]
fn every_variable_is_the_key_in_force_over_a_file_that_says_nothing() {
    for field in Config::SCHEMA {
        let (_, variable, file) = worked(field.name);
        let from_a_silent_file = resolved_in("", &[(field.variable, variable)], Config::default());

        assert_eq!(
            from_a_silent_file,
            the_key_alone(field.name, file),
            "`{}` set over a file that says nothing is the key in force, replacing rather than adding",
            field.variable
        );
    }
}

#[test]
fn every_key_merges_above_a_lower_config_and_yields_to_one_above_it() {
    let full = file_read(&examples_document());

    assert_eq!(
        full.clone().above(Config::default()),
        full,
        "a key the higher config names is the key in force, and it is the higher config's value"
    );
    assert_eq!(
        Config::default().above(full.clone()),
        full,
        "a key the higher config says nothing about is the lower config's key"
    );
}

#[test]
fn every_key_is_parsed_shown_written_spoken_and_merged_by_its_own_row() {
    assert_eq!(
        SPOKEN.len(),
        Config::SCHEMA.len(),
        "the worked examples and the key list must name the same keys"
    );

    let mut names: Vec<&str> = Vec::new();
    let mut variables: Vec<&str> = Vec::new();
    let mut masked: Vec<&str> = Vec::new();

    for field in Config::SCHEMA {
        let key = field.name;

        assert!(
            !key.is_empty(),
            "every row of the table carries a name, and this row carries none"
        );
        assert!(
            !names.contains(&key),
            "no two rows of the table name one key, and `{key}` is named twice"
        );
        names.push(key);

        let (_, spoken, file) = worked(key);
        let alone = the_key_alone(key, file);

        let words = field.shown(&alone);
        assert!(
            !words.is_empty(),
            "the file says a value for `{key}`, so the row of `{key}` shows one"
        );
        assert!(
            field.shown(&Config::default()).is_empty(),
            "a config that says nothing about `{key}` is the row of `{key}` showing nothing"
        );

        let typed = if words == sindri_core::redact::MASK {
            spoken
        } else {
            words.as_str()
        };
        assert_eq!(
            field
                .read(typed)
                .unwrap_or_else(|error| panic!("the row of `{key}` parses `{typed}`: {error}")),
            alone,
            "the words the row of `{key}` is handed are `{key}`'s value and no other key's"
        );
        if words == sindri_core::redact::MASK {
            masked.push(key);
            let refused = field.read(&words).expect_err(&format!(
                "the row of `{key}` shows the mask, so it refuses the mask as a value"
            ));
            assert!(
                refused.to_string().contains(key),
                "the refusal of the mask names the key a reader has to fix, and `{key}`'s says: \
                 {refused}"
            );
        }

        let mut document = toml_edit::DocumentMut::new();
        field
            .write(&alone, &mut document)
            .unwrap_or_else(|error| panic!("the row of `{key}` writes its key: {error}"));
        let written = document.to_string();
        let table: toml::Table = written
            .parse()
            .unwrap_or_else(|error| panic!("the document of `{key}` is TOML: {error}"));
        let named: Vec<&str> = table.keys().map(String::as_str).collect();
        assert_eq!(
            named,
            [key],
            "the row of `{key}` writes that key and no other key, and the document is:\n{written}"
        );
        let back: Config = toml::from_str(&written).unwrap_or_else(|error| {
            panic!("the document the row of `{key}` writes reads back: {error}")
        });
        assert_eq!(
            back, alone,
            "the document the row of `{key}` writes reads back as `{key}`'s value and no other, \
             and the document is:\n{written}"
        );

        assert!(
            !field.variable.is_empty(),
            "the row of `{key}` carries the variable that speaks it"
        );
        assert!(
            !variables.contains(&field.variable),
            "no two rows of the table are spoken by one variable, and `{}` is not `{key}`'s alone",
            field.variable
        );
        variables.push(field.variable);
        let mut from_the_variable = Config::default();
        field
            .speak(spoken, &mut from_the_variable)
            .unwrap_or_else(|error| {
                panic!("`{}` speaks what `{key}` holds: {error}", field.variable)
            });
        assert_eq!(
            from_the_variable, alone,
            "`{}` spoken into a config that says nothing is `{key}`'s value and no other key's",
            field.variable
        );

        let mut above = alone.clone();
        field.or_lower(&mut above, &Config::default());
        assert_eq!(
            above, alone,
            "`{key}` named above a config that says nothing keeps its own value"
        );
        let mut below = Config::default();
        field.or_lower(&mut below, &alone);
        assert_eq!(
            below, alone,
            "`{key}` named nowhere above a config that says it takes the lower one's value"
        );
    }

    assert_eq!(
        masked,
        ["api_key"],
        "the table masks the one row a secret is drawn as, so no row shows a secret"
    );
}
