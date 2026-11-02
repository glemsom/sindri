//! The `bash` tool's contract with the model: one tool, a description derived
//! from the sandbox policy and the limits in force rather than written by hand,
//! and a structured result rendered with every field promised.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::agent::{BashTool, Budget, Limits};
use sindri_core::exec::{Outcome, ToolResult};
use sindri_core::provider::Call;
use support::{Scripted, fixtures};

#[test]
fn the_model_gets_exactly_one_tool_named_bash_described_by_the_policy() {
    let (workspace, _session_temp, policy) = fixtures();
    let tool = BashTool::new(&policy, Limits::default(), Scripted::silent());

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
    for promised in [
        "put the steps of one task in one script",
        "$TMPDIR",
        "retained_bytes",
    ] {
        assert!(
            description.contains(promised),
            "the description says `{promised}`: {description}"
        );
    }
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["required"][0], "command");
}

#[test]
fn the_description_costs_at_most_160_words_besides_the_policy() {
    let (_workspace, _session_temp, policy) = fixtures();
    let tool = BashTool::new(&policy, Limits::default(), Scripted::silent());

    // The policy is derived and its length is the sandbox's business; the tax
    // that can creep back unnoticed is the prose written around it. The limits
    // sentence is the ~30 words this ceiling pays for: what `Budget::default()`
    // and the timeout are is a fact about this program, not about bash.
    let prose = tool.spec().description.replacen(&policy.describe(), "", 1);
    let words = prose.split_whitespace().count();
    assert!(
        words <= 160,
        "the description is re-sent every turn, so it stays at most 160 words of prose around the policy, not {words}: {prose}"
    );
}

#[test]
fn the_description_leaves_shell_semantics_and_field_names_to_the_results() {
    let (_workspace, _session_temp, policy) = fixtures();
    let tool = BashTool::new(&policy, Limits::default(), Scripted::silent());
    let description = &tool.spec().description;

    // A name the model can read off its first result is not a word worth
    // sending on every turn; what a name cannot say stays above.
    for taught in [
        "set -e",
        "pipefail",
        "exit status",
        "duration_ms",
        "truncated",
        "stdout",
        "stderr",
    ] {
        assert!(
            !description.contains(taught),
            "the description does not teach `{taught}`: {description}"
        );
    }
}

#[test]
fn the_description_names_the_limits_that_end_a_run_from_the_values_in_force() {
    let (_workspace, _session_temp, policy) = fixtures();
    let limits = Limits {
        command_timeout: Duration::from_secs(7),
        budget: Budget {
            wall: Duration::from_secs(90),
            bytes: 4096,
        },
    };
    let tool = BashTool::new(&policy, limits, Scripted::silent());
    let description = &tool.spec().description;

    // The numbers are formatted from the values, never written by hand, so a
    // config that shortens the timeout shortens the sentence with it.
    for named in [
        "killed after 7 s",
        "90 s of command time",
        "4 KiB of output",
    ] {
        assert!(
            description.contains(named),
            "the description says `{named}`: {description}"
        );
    }
}

#[test]
fn the_default_description_names_the_defaults_the_run_actually_enforces() {
    let (_workspace, _session_temp, policy) = fixtures();
    let tool = BashTool::new(&policy, Limits::default(), Scripted::silent());
    let description = &tool.spec().description;

    for named in ["killed after 120 s", "one hour of command time", "8 MiB"] {
        assert!(
            description.contains(named),
            "the description says `{named}`: {description}"
        );
    }
    assert!(
        description.contains("timed_out"),
        "the model is told what a killed command is reported as: {description}"
    );
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
    let tool = BashTool::new(&policy, Limits::default(), executor);

    let outcome = tool
        .call(&command_call("call_1", "ls | wc -l"), "ls | wc -l")
        .expect("the run was not cancelled");

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
        // What this result costs the conversation, in the bytes it carries.
        "retained_bytes: 21", // "twelve files" and "a warning", as the model is charged
        "stdout:\ntwelve files",
        "stderr:\na warning",
    ] {
        assert!(
            content.contains(expected),
            "the model is told `{expected}`: {content}"
        );
    }
    assert!(
        content.contains(&format!(
            "{} (grep it from the next script)",
            spill.display()
        )),
        "the model is told where the untruncated text went, and to read it from \
         inside the next script rather than print it again: {content}"
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
    let tool = BashTool::new(&policy, Limits::default(), executor);

    let outcome = tool
        .call(&command_call("call_1", "true"), "true")
        .expect("the run was not cancelled");

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
