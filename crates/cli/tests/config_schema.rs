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

/// A directory the run may be granted writable, existing and canonical so the
/// sandbox accepts it.
fn extra_dir(env: &Env, name: &str) -> std::path::PathBuf {
    let dir = env.workspace.parent().expect("root").join(name);
    std::fs::create_dir_all(&dir).expect("extra dir");
    dir.canonicalize().expect("canonical extra")
}

#[test]
fn the_file_grants_an_extra_writable_path_to_both_enforcement_and_description() {
    let env = Env::new();
    let extra = extra_dir(&env, "shared");
    write_config(
        &env,
        &format!(
            "model = \"claude-from-file\"\nextra_writable_paths = [\"{}\"]\n",
            extra.display()
        ),
    );

    let mock = MockProvider::start(vec![
        Reply::tool_call(
            "t1",
            &format!("touch {}/from-the-agent", extra.display()),
            "",
        ),
        Reply::answer("done"),
    ]);
    let output = run(
        &env,
        &mock,
        &["-b", "write over there"],
        &[("SINDRI_API_KEY", TEST_KEY)],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let body = mock.requests()[0].json().to_string();
    assert!(
        body.contains(&extra.display().to_string()),
        "the model is told the extra path is writable: {body}"
    );
    assert!(
        extra.join("from-the-agent").is_file(),
        "the extra path really is writable, not merely described"
    );
}

#[test]
fn the_environment_and_a_flag_override_the_files_extra_paths() {
    let env = Env::new();
    let from_file = extra_dir(&env, "from-file");
    let from_env = extra_dir(&env, "from-env");
    let from_flag = extra_dir(&env, "from-flag");
    write_config(
        &env,
        &format!(
            "model = \"claude-from-file\"\nextra_writable_paths = [\"{}\"]\n",
            from_file.display()
        ),
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &["-b", "summarise this"],
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            (
                "SINDRI_EXTRA_WRITABLE_PATHS",
                &from_env.display().to_string(),
            ),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let body = only_request(&mock).json().to_string();
    assert!(
        body.contains(&from_env.display().to_string())
            && !body.contains(&from_file.display().to_string()),
        "the environment replaces the file's extra paths rather than adding to them: {body}"
    );

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = run(
        &env,
        &mock,
        &[
            "-b",
            "summarise this",
            "--writable",
            &from_flag.display().to_string(),
        ],
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            (
                "SINDRI_EXTRA_WRITABLE_PATHS",
                &from_env.display().to_string(),
            ),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let body = only_request(&mock).json().to_string();
    assert!(
        body.contains(&from_flag.display().to_string())
            && !body.contains(&from_env.display().to_string()),
        "the flag replaces the environment's extra paths: {body}"
    );
}

#[test]
fn an_extra_path_that_could_not_be_mounted_is_a_usage_error() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);

    for bad in ["relative/dir", "/", "/nonexistent-sindri-extra"] {
        let output = run(
            &env,
            &mock,
            &["-b", "summarise this", "--writable", bad],
            &[
                ("SINDRI_API_KEY", TEST_KEY),
                ("SINDRI_MODEL", "claude-sonnet-4-6"),
            ],
        );
        assert_eq!(
            output.status.code(),
            Some(2),
            "`{bad}` is a usage error: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(
            message.contains(bad),
            "the error names the path `{bad}`: {message}"
        );
    }
    assert_eq!(mock.requests().len(), 0, "no run leaves the machine");
}

#[test]
fn a_credential_from_the_file_is_redacted_from_errors() {
    const FROM_FILE: &str = "sk-ant-from-the-config-file";
    let env = Env::new();
    write_config(
        &env,
        &format!("model = \"claude-from-file\"\napi_key = \"{FROM_FILE}\"\n"),
    );

    let mock = MockProvider::start(vec![Reply::error(
        401,
        "authentication_error",
        &format!("invalid x-api-key `{FROM_FILE}`"),
    )]);
    let output = run(&env, &mock, &["-b", "summarise this"], &[]);

    assert_eq!(output.status.code(), Some(1), "an auth failure is exit 1");
    let message = stderr(&output);
    assert!(
        !message.contains(FROM_FILE),
        "the file's credential is never printed: {message}"
    );
    assert!(
        message.contains("[redacted]"),
        "the credential is replaced, not dropped: {message}"
    );
}
