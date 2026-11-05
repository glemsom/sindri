//! The headers a run sends besides the ones the wire writes: a gateway that
//! routes on a header of its own cannot be reached without one, and the value
//! it wants is as likely to be a secret as the credential is.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, stderr, write_config};

fn run(
    env: &Env,
    mock: &MockProvider,
    args: &[&str],
    vars: &[(&str, &str)],
) -> std::process::Output {
    let base_url = format!("{}/v1", mock.base_url());
    let mut all: Vec<(&str, &str)> = vec![
        ("SINDRI_PROVIDER", "openai"),
        ("SINDRI_BASE_URL", base_url.as_str()),
        ("SINDRI_MODEL", "kimi-k3"),
    ];
    all.extend(vars.iter().copied());
    env.cmd()
        .args(args)
        .envs(all)
        .write_stdin("--- a/x\n+++ b/x\n")
        .assert()
        .get_output()
        .clone()
}

fn headers(
    env: &Env,
    mock: &MockProvider,
    args: &[&str],
    vars: &[(&str, &str)],
) -> std::process::Output {
    let mut all = vec!["-b", "summarise this"];
    all.extend(args.iter().copied());
    run(env, mock, &all, vars)
}

#[test]
fn a_header_the_gateway_routes_on_reaches_the_server() {
    let env = Env::new();
    write_config(
        &env,
        r#"
[extra_headers]
x-opencode-session = "a-session"
"#,
    );
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(&env, &mock, &[], &[]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        mock.requests()[0].header("x-opencode-session"),
        Some("a-session"),
        "a gateway that routes on its own header cannot be reached without it"
    );
}

#[test]
fn a_header_on_the_command_line_replaces_the_configured_ones() {
    let env = Env::new();
    write_config(
        &env,
        r#"
[extra_headers]
x-opencode-session = "from-the-file"
x-from-the-file = "kept-out"
"#,
    );
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(
        &env,
        &mock,
        &[
            "--header",
            "x-opencode-session: from-the-flag",
            "--header",
            "x-other: also-the-flag",
        ],
        &[("SINDRI_EXTRA_HEADERS", "x-opencode-session: from-the-env")],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let request = &mock.requests()[0];
    assert_eq!(
        request.header("x-opencode-session"),
        Some("from-the-flag"),
        "the flag wins over the environment and the file"
    );
    assert_eq!(
        request.header("x-other"),
        Some("also-the-flag"),
        "every header the flag names is sent"
    );
    assert_eq!(
        request.header("x-from-the-file"),
        None,
        "the level that names headers replaces the rest rather than adding to them"
    );
}

#[test]
fn a_header_the_environment_names_needs_no_config_file() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(
        &env,
        &mock,
        &[],
        &[("SINDRI_EXTRA_HEADERS", "x-opencode-session: from-the-env")],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        mock.requests()[0].header("x-opencode-session"),
        Some("from-the-env")
    );
}

#[test]
fn a_header_with_no_value_is_a_usage_error_that_echoes_nothing() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(&env, &mock, &["--header", "s3cr3t-with-no-colon"], &[]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "a malformed header is a usage error: {}",
        stderr(&output)
    );
    let message = stderr(&output);
    assert!(
        message.contains("extra_headers"),
        "the error names the setting: {message}"
    );
    assert!(
        !message.contains("s3cr3t"),
        "the error does not echo what may be a secret: {message}"
    );
    assert_eq!(mock.requests().len(), 0, "a broken header sends nothing");
}

#[test]
fn a_header_the_wire_owns_is_refused_rather_than_silently_overridden() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(&env, &mock, &["--header", "Content-Type: text/plain"], &[]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains("content-type"),
        "the error names the header, lowercased: {message}"
    );
    assert_eq!(mock.requests().len(), 0, "a refused header sends nothing");
}

/// The id of the session the run left a record under, as the record itself
/// writes it: a batch run opens exactly one, in the state root the test gave it.
fn session_id(env: &Env) -> String {
    let dir = env.state.join("sindri/sessions");
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .collect();
    found.sort();
    assert_eq!(found.len(), 1, "a batch run opens one session: {found:?}");
    let record = std::fs::read_to_string(found[0].join("session.json"))
        .unwrap_or_else(|error| panic!("{}: {error}", found[0].display()));
    let meta: serde_json::Value = serde_json::from_str(&record).expect("a record's metadata");
    let id = meta["id"]
        .as_str()
        .expect("the record names the session it was written for")
        .to_string();
    assert_eq!(
        found[0].file_name().and_then(|name| name.to_str()),
        Some(id.as_str()),
        "the id the record states is the directory it sits in"
    );
    id
}

#[test]
fn a_session_token_in_a_configured_header_is_the_id_of_the_session_the_run_left() {
    let env = Env::new();
    write_config(
        &env,
        r#"
[extra_headers]
x-opencode-session = "{session}"
"#,
    );
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(&env, &mock, &[], &[]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        mock.requests()[0].header("x-opencode-session"),
        Some(session_id(&env).as_str()),
        "a gateway that caches per session is told the id the run recorded under, \
         not the token it was written with"
    );
}

#[test]
fn a_session_token_on_the_flag_is_the_session_id_and_replaces_the_environment_level() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(
        &env,
        &mock,
        &["--header", "x-opencode-session: {session}"],
        &[("SINDRI_EXTRA_HEADERS", "x-env-only: {session}")],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let request = &mock.requests()[0];
    assert_eq!(
        request.header("x-opencode-session"),
        Some(session_id(&env).as_str()),
        "the flag's value carries the token like the file's, and the flag still wins"
    );
    assert_eq!(
        request.header("x-env-only"),
        None,
        "the level that names headers replaces the rest, so the environment's line is not sent"
    );
}

#[test]
fn a_session_token_in_a_header_the_environment_names_is_the_session_id() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = headers(
        &env,
        &mock,
        &[],
        &[("SINDRI_EXTRA_HEADERS", "x-opencode-session: {session}")],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        mock.requests()[0].header("x-opencode-session"),
        Some(session_id(&env).as_str())
    );
}

#[test]
fn every_turn_of_one_run_is_sent_with_that_session_id() {
    let env = Env::new();
    write_config(
        &env,
        r#"
[extra_headers]
x-opencode-session = "{session}"
"#,
    );
    // The first turn asks for a command and the second answers, so this is one
    // run that asks the wire twice.
    let mock = MockProvider::start(vec![
        Reply::recorded("chat-tool-call", 200),
        Reply::recorded("chat-answer", 200),
    ]);

    let output = run(&env, &mock, &["-b", "say hello"], &[]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let id = session_id(&env);
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "one turn to call, one to answer");
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(
            request.header("x-opencode-session"),
            Some(id.as_str()),
            "turn {} of the run is sent with the id its record carries",
            index + 1
        );
    }
}
