//! The process seam for configuration: file, then environment, then flags
//! (ADR-0005), proven by what the provider is actually sent, and the two doors
//! a bad setting leaves by — exit 2 when it is wrong, exit 1 when it is absent.

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

/// Runs batch mode with exactly the settings the test supplies, so each test
/// states its own precedence rather than inheriting the harness's defaults.
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
fn the_config_file_supplies_the_model_and_the_credential() {
    let env = Env::new();
    write_config(
        &env,
        r#"
model = "claude-from-file"
api_key = "sk-ant-from-file"
"#,
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(&env, &mock, &["-b", "summarise this"], &[]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let request = only_request(&mock);
    assert!(
        request.body.contains("claude-from-file"),
        "the file's model is used when nothing overrides it: {}",
        request.body
    );
    assert_eq!(
        request.header("x-api-key"),
        Some("sk-ant-from-file"),
        "the file's credential is used when nothing overrides it"
    );
}

#[test]
fn the_environment_overrides_the_file_and_a_flag_overrides_the_environment() {
    let env = Env::new();
    write_config(
        &env,
        r#"
model = "claude-from-file"
api_key = "sk-ant-from-file"
"#,
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[
            ("SINDRI_MODEL", "claude-from-env"),
            ("SINDRI_API_KEY", TEST_KEY),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let request = only_request(&mock);
    assert!(
        request.body.contains("claude-from-env"),
        "the environment overrides the file: {}",
        request.body
    );
    assert_eq!(
        request.header("x-api-key"),
        Some(TEST_KEY),
        "the environment's credential overrides the file's"
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this", "--model", "claude-from-flag"],
        &[
            ("SINDRI_MODEL", "claude-from-env"),
            ("SINDRI_API_KEY", TEST_KEY),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        only_request(&mock).body.contains("claude-from-flag"),
        "the flag overrides the environment"
    );
}

#[test]
fn a_missing_model_or_credential_exits_one_with_a_hint() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);

    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );
    assert_eq!(output.status.code(), Some(1), "no model is exit 1");
    assert_eq!(stdout(&output), "", "a missing setting writes no answer");
    let hint = stderr(&output);
    assert!(hint.contains("model"), "the hint names the model: {hint}");
    assert!(
        hint.contains("--model") && hint.contains("SINDRI_MODEL"),
        "the hint names how to set it: {hint}"
    );

    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[("SINDRI_MODEL", "claude-sonnet-4-6")],
    );
    assert_eq!(output.status.code(), Some(1), "no credential is exit 1");
    let hint = stderr(&output);
    assert!(
        hint.contains("ANTHROPIC_API_KEY"),
        "the hint names the variable to set: {hint}"
    );
    assert_eq!(mock.requests().len(), 0, "a missing setting sends nothing");
}

#[test]
fn a_wrong_setting_is_a_usage_error() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);

    for args in [
        &["-b", "q", "--timeout", "0"][..],
        &["-b", "q", "--timeout", "3601"],
        &["-b", "q", "--max-turns", "0"],
        &["-b", "q", "--provider", "gemini"],
    ] {
        let output = run(
            &env,
            &mock,
            args,
            &[
                ("SINDRI_MODEL", "claude-sonnet-4-6"),
                ("SINDRI_API_KEY", TEST_KEY),
            ],
        );
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?} is a usage error: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), "", "{args:?} writes nothing to stdout");
    }

    let output = run(
        &env,
        &mock,
        &["-b", "q"],
        &[
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_TIMEOUT", "0"),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "an unusable environment value is refused, not silently defaulted: {}",
        stderr(&output)
    );
}

#[test]
fn a_flag_overrides_a_wrong_environment_value() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &[
            "-b",
            "summarise this",
            "--timeout",
            "30",
            "--max-turns",
            "5",
        ],
        &[
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_TIMEOUT", "0"),
            ("SINDRI_MAX_TURNS", "0"),
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "the flags override the environment: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "ok\n");
}

#[test]
fn an_unreadable_config_file_is_a_usage_error() {
    let env = Env::new();
    write_config(&env, "model = [not toml");
    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
            ("SINDRI_API_KEY", TEST_KEY),
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(2),
        "a broken config file is a usage error: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("config.toml"),
        "the error names the file: {}",
        stderr(&output)
    );
}
