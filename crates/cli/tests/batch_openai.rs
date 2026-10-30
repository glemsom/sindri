//! The process seam for the OpenAI-compatible wire (ADR-0003): one binary, one
//! agent, whether the models run on the user's own machine or behind an
//! aggregator. The wire is a base URL, so the same code serves
//! `http://localhost:11434/v1` and `https://openrouter.ai/api/v1`.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, TEST_KEY, stderr, stdout, write_config};

/// The request an OpenAI-compatible server is asked: `POST {base}/chat/completions`.
const CHAT_PATH: &str = "/v1/chat/completions";

fn run(env: &Env, args: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = env.cmd();
    cmd.envs(args.iter().copied())
        .args(["-b", "summarise this"])
        .write_stdin("--- a/x\n+++ b/x\n")
        .assert()
        .get_output()
        .clone()
}

fn run_json(env: &Env, args: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = env.cmd();
    cmd.envs(args.iter().copied())
        .args(["-b", "summarise this", "--format", "json"])
        .write_stdin("--- a/x\n+++ b/x\n")
        .assert()
        .get_output()
        .clone()
}

#[test]
fn a_local_server_needs_no_credential_and_is_asked_at_its_base_url() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);
    let base_url = format!("{}/v1", mock.base_url());

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &base_url),
            ("SINDRI_MODEL", "llama3.2"),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Two files changed.\n",
        "the answer, one newline"
    );

    let request = &mock.requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, CHAT_PATH);
    assert_eq!(request.json()["model"], "llama3.2");
    assert_eq!(
        request.header("authorization"),
        None,
        "a server that wants no credential is not given a blank one"
    );
}

#[test]
fn an_aggregator_base_url_keeps_its_path_prefix_and_its_credential() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);
    let base_url = format!("{}/api/v1", mock.base_url());

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &base_url),
            ("SINDRI_MODEL", "meta-llama/llama-3.2-3b-instruct"),
            ("OPENAI_API_KEY", TEST_KEY),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let request = &mock.requests()[0];
    assert_eq!(
        request.path, "/api/v1/chat/completions",
        "the configured path is the one asked"
    );
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str()),
        "a hosted aggregator is given the credential it asks for"
    );
    assert_eq!(request.json()["model"], "meta-llama/llama-3.2-3b-instruct");
}

#[test]
fn no_model_configured_names_the_wire_and_where_a_model_comes_from() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);
    let base_url = format!("{}/v1", mock.base_url());

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &base_url),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "no model is a model error");
    assert_eq!(stdout(&output), "", "a missing setting writes no answer");
    let hint = stderr(&output);
    assert!(
        hint.contains("openai"),
        "the error names the wire it is missing a model for: {hint}"
    );
    assert!(
        hint.contains("--model") && hint.contains("SINDRI_MODEL"),
        "the error names how to set it: {hint}"
    );
    assert!(
        hint.contains("/models"),
        "the error says where a server's models are read from: {hint}"
    );
    assert_eq!(mock.requests().len(), 0, "a missing setting sends nothing");
}

#[test]
fn the_bash_tool_is_called_over_the_wire_and_its_result_goes_back() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::recorded("chat-tool-call", 200),
        Reply::recorded("chat-answer", 200),
    ]);
    let base_url = format!("{}/v1", mock.base_url());

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &base_url),
            ("SINDRI_MODEL", "llama3.2"),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Two files changed.\n");

    let requests = mock.requests();
    assert_eq!(
        requests.len(),
        2,
        "the call is answered before the next turn"
    );
    let tools = &requests[0].json()["tools"];
    assert_eq!(
        tools[0]["function"]["name"], "bash",
        "the one tool is offered in the wire's own shape: {tools}"
    );
    assert!(
        tools[0]["function"]["description"]
            .as_str()
            .is_some_and(|description| !description.is_empty()),
        "the sandbox's description is the model's to read: {tools}"
    );

    let messages = requests[1].json()["messages"].clone();
    let answer = messages
        .as_array()
        .and_then(|messages| messages.last())
        .cloned()
        .expect("the tool result is the last message");
    assert_eq!(answer["role"], "tool");
    assert_eq!(answer["tool_call_id"], "call_abc123");
    assert!(
        answer["content"]
            .as_str()
            .is_some_and(|text| text.contains("hi")),
        "the command's own output is what the model is told: {answer}"
    );
}

#[test]
fn a_server_without_tool_calling_is_reported_plainly() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("tools-unsupported", 400)]);
    let base_url = format!("{}/v1", mock.base_url());

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &base_url),
            ("SINDRI_MODEL", "gemma:2b"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "a refusal is a model error");
    assert_eq!(stdout(&output), "", "a failure writes no answer");
    let stderr = stderr(&output);
    assert!(
        stderr.contains("does not support tool calling"),
        "the failure says what the server cannot do: {stderr}"
    );
    assert!(
        stderr.contains("does not support tools"),
        "the server's own words are kept: {stderr}"
    );
    assert_eq!(
        mock.requests().len(),
        1,
        "a refusal is not retried: it would be refused identically"
    );
}

#[test]
fn usage_and_cached_tokens_are_reported_where_the_server_reports_them() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-usage-cached", 200)]);

    let output = run_json(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &format!("{}/v1", mock.base_url())),
            ("SINDRI_MODEL", "llama3.2"),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let object: serde_json::Value =
        serde_json::from_str(stdout(&output).trim_end()).expect("one JSON object");
    let usage = &object["usage"];
    assert_eq!(usage["input_tokens"], 1024, "the prompt's tokens: {usage}");
    assert_eq!(usage["output_tokens"], 33, "the answer's tokens: {usage}");
    assert_eq!(
        usage["total_tokens"], 1057,
        "the server's own total: {usage}"
    );
    assert_eq!(
        usage["cached_input_tokens"], 896,
        "a server that names its cached tokens has them reported: {usage}"
    );
}

#[test]
fn reasoning_is_asked_for_only_when_the_flag_asks() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);
    let base_url = format!("{}/v1", mock.base_url());
    let settings = [
        ("SINDRI_PROVIDER", "openai"),
        ("SINDRI_BASE_URL", base_url.as_str()),
        ("SINDRI_MODEL", "llama3.2"),
    ];

    let output = run(&env, &settings);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let mut cmd = env.cmd();
    let output = cmd
        .envs(settings.iter().copied())
        .args(["-b", "summarise this", "--thinking"])
        .write_stdin("--- a/x\n+++ b/x\n")
        .assert()
        .get_output()
        .clone();
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let requests = mock.requests();
    assert_eq!(
        requests[0].json().get("reasoning_effort"),
        None,
        "a run that did not ask for reasoning does not buy it: {}",
        requests[0].body
    );
    assert!(
        requests[1].json().get("reasoning_effort").is_some(),
        "the flag asks this wire for the effort level it takes: {}",
        requests[1].body
    );
}

#[test]
fn the_wire_and_its_base_url_can_be_configured_in_the_file() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);
    write_config(
        &env,
        &format!(
            "provider = \"openai\"\nmodel = \"llama3.2\"\nbase_url = \"{}/v1\"\n",
            mock.base_url()
        ),
    );

    let output = run(&env, &[]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        mock.requests()[0].json()["model"],
        "llama3.2",
        "the file configured the wire, its base URL and its model"
    );
}

#[test]
fn the_providers_own_variables_configure_the_wire() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("chat-answer", 200)]);

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_MODEL", "llama3.2"),
            ("OPENAI_BASE_URL", &format!("{}/v1", mock.base_url())),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(mock.requests()[0].path, CHAT_PATH);
}

#[test]
fn a_rejected_tool_call_is_not_mistaken_for_a_server_without_tools() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::recorded("tool-call-rejected", 400)]);

    let output = run(
        &env,
        &[
            ("SINDRI_PROVIDER", "openai"),
            ("SINDRI_BASE_URL", &format!("{}/v1", mock.base_url())),
            ("SINDRI_MODEL", "llama3.2"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "a refusal is a model error");
    let stderr = stderr(&output);
    assert!(
        stderr.contains("not valid JSON"),
        "the server's complaint reaches the user: {stderr}"
    );
    assert!(
        !stderr.contains("does not support tool calling"),
        "a rejected call is not a missing capability: {stderr}"
    );
}
