#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

//! The `bash` tool's contract with the model: one tool, a description derived
//! from the sandbox policy rather than written by hand (ADR-0002), and a
//! structured result rendered with every field ADR-0007 promises.

mod support;

use std::sync::{Arc, Mutex};

use sindri_core::agent::BashTool;
use sindri_core::exec::{Outcome, ToolResult};
use sindri_core::provider::Call;
use support::{Scripted, fixtures};

#[test]
fn the_model_gets_exactly_one_tool_named_bash_described_by_the_policy() {
    let (workspace, _session_temp, policy) = fixtures();
    let tool = BashTool::new(&policy, Scripted::silent());

    let spec = tool.spec();
    assert_eq!(spec.name, "bash", "the only tool is bash");
    let description = &spec.description;
    assert!(
        description.contains(&policy.describe()),
        "the description is generated from the policy, not written by hand: {description}"
    );
    assert!(
        description.contains(&workspace.path().display().to_string()),
        "the model is told which paths it may write: {description}"
    );
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["required"][0], "command");
}

#[test]
fn a_tool_result_reaches_the_model_with_every_field_it_needs() {
    let (_workspace, session_temp, policy) = fixtures();
    let spill = session_temp.join("spill/bash-1.out");
    std::fs::create_dir_all(spill.parent().expect("spill dir")).expect("spill dir");
    std::fs::write(&spill, "the whole story").expect("spill");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let executor = Arc::new(Scripted::new(
        vec![ToolResult {
            outcome: Outcome::Exited { exit_code: 7 },
            stdout: "twelve files".to_string(),
            stderr: "a warning".to_string(),
            truncated: true,
            spill: vec![spill.clone()],
            duration_ms: 42,
        }],
        Arc::clone(&seen),
    ));
    let tool = BashTool::new(&policy, executor);

    let outcome = tool.call(&command_call("call_1", "ls | wc -l"));

    assert_eq!(*seen.lock().expect("seen"), vec!["ls | wc -l"]);
    assert_eq!(outcome.call_id, "call_1");
    assert!(
        !outcome.is_error,
        "a non-zero exit is a result, not a tool error"
    );
    let content = &outcome.content;
    for expected in [
        "outcome: exited",
        "exit_code: 7",
        "duration_ms: 42",
        "truncated: true",
        "stdout:\ntwelve files",
        "stderr:\na warning",
    ] {
        assert!(
            content.contains(expected),
            "the model is told `{expected}`: {content}"
        );
    }
    assert!(
        content.contains(&spill.display().to_string()),
        "the model is told where the untruncated text went: {content}"
    );
}

#[test]
fn a_call_the_sandbox_never_started_is_reported_as_a_tool_error() {
    let (_workspace, _session_temp, policy) = fixtures();
    let executor = Arc::new(Scripted::new(
        vec![ToolResult {
            outcome: Outcome::SandboxError {
                detail: "setting up uid map: Permission denied".to_string(),
            },
            stdout: String::new(),
            stderr: String::new(),
            truncated: false,
            spill: Vec::new(),
            duration_ms: 2,
        }],
        Arc::new(Mutex::new(Vec::new())),
    ));
    let tool = BashTool::new(&policy, executor);

    let outcome = tool.call(&command_call("call_1", "true"));

    assert!(
        outcome.is_error,
        "a sandbox that never started is the tool failing, not the command"
    );
    assert!(
        outcome.content.contains("outcome: sandbox_error")
            && outcome.content.contains("Permission denied"),
        "the model is told the sandbox refused: {}",
        outcome.content
    );
}

/// A well-formed call to `bash`.
pub fn command_call(id: &str, command: &str) -> Call {
    Call::new(
        id,
        "bash",
        Ok(serde_json::Map::from_iter([(
            "command".to_string(),
            serde_json::json!(command),
        )])),
    )
}
