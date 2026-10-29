//! The config file's schema, proven at the process seam: an unknown key is an
//! error naming the key and the file, and every setting the schema promises is
//! set from the file and overridden by the environment and then by flags
//! (ADR-0005).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::process::Output;

use common::provider::{MockProvider, Reply};
use common::{Env, TEST_KEY, stderr, stdout};

/// Writes `$XDG_CONFIG_HOME/sindri/config.toml`.
fn write_config(env: &Env, toml: &str) {
    let dir = env.config.join("sindri");
    std::fs::create_dir_all(&dir).expect("config dir");
    std::fs::write(dir.join("config.toml"), toml).expect("write config");
}

/// Runs batch mode with only the settings the test supplies, alongside the
/// mock's base URL so no request can leave the machine.
fn run(env: &Env, mock: &MockProvider, args: &[&str], vars: &[(&str, &str)]) -> Output {
    let mut cmd = env.cmd();
    cmd.args(args)
        .env("SINDRI_BASE_URL", mock.base_url())
        .envs(vars.iter().copied())
        .write_stdin("")
        .assert()
        .get_output()
        .clone()
}

/// The one request the mock served.
fn only_request(mock: &MockProvider) -> common::provider::Request {
    let requests = mock.requests();
    assert_eq!(requests.len(), 1, "one run makes one request");
    requests[0].clone()
}

#[test]
fn an_unknown_key_is_an_error_naming_the_key_and_the_file() {
    let env = Env::new();
    write_config(
        &env,
        r#"
model = "claude-from-file"
max_turn = 5
"#,
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );

    assert_eq!(
        output.status.code(),
        Some(2),
        "a typo'd key is refused, not ignored: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "", "a broken config writes no answer");
    let message = stderr(&output);
    assert!(
        message.contains("max_turn"),
        "the error names the key: {message}"
    );
    assert!(
        message.contains("config.toml"),
        "the error names the file: {message}"
    );
    assert_eq!(
        mock.requests().len(),
        0,
        "a config that cannot be read sends nothing"
    );
}

/// Whether the request paid for reasoning.
fn reasoning(request: &common::provider::Request) -> bool {
    request.json().get("thinking").is_some()
}

#[test]
fn the_file_can_turn_reasoning_on_and_the_environment_and_a_flag_win() {
    let env = Env::new();
    write_config(
        &env,
        r#"
model = "claude-from-file"
thinking = true
"#,
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        reasoning(&only_request(&mock)),
        "the file can turn reasoning on"
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY), ("SINDRI_THINKING", "false")],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !reasoning(&only_request(&mock)),
        "the environment overrides the file's reasoning toggle"
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this", "--thinking"],
        &[("SINDRI_API_KEY", TEST_KEY), ("SINDRI_THINKING", "false")],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        reasoning(&only_request(&mock)),
        "the flag overrides the environment's reasoning toggle"
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this", "--no-thinking"],
        &[("SINDRI_API_KEY", TEST_KEY), ("SINDRI_THINKING", "true")],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !reasoning(&only_request(&mock)),
        "--no-thinking overrides the environment's toggle too"
    );
}

#[test]
fn a_wrong_reasoning_toggle_is_a_usage_error() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
            ("SINDRI_THINKING", "yes please"),
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(2),
        "a toggle that is neither true nor false is refused: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("SINDRI_THINKING"),
        "the error names the variable: {}",
        stderr(&output)
    );
}
