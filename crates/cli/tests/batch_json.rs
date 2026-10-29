//! The process seam for `--format json` (ADR-0004): one compact object on one
//! line, a field order that never changes, absent data absent rather than
//! null, and exit codes that do not care how the run was printed.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::Env;
use common::provider::{MockProvider, Reply};
use common::{batch, stderr, stdout};

/// The contract's field order. Later fields append; this order never changes.
const FIELD_ORDER: [&str; 8] = [
    "answer",
    "session",
    "turns",
    "stopped",
    "model",
    "duration_ms",
    "tool_calls",
    "usage",
];

/// One compact object, one line, and the contract's fields in its order.
fn one_ordered_object(output: &std::process::Output) -> String {
    let stdout = stdout(output);
    assert_eq!(
        stdout.matches('\n').count(),
        1,
        "one line holds the whole object: {stdout:?}"
    );
    assert!(
        stdout.ends_with("}\n"),
        "the object is the line, plus one newline: {stdout:?}"
    );

    let line = stdout.trim_end_matches('\n').to_string();
    let mut cursor = 0;
    for field in FIELD_ORDER {
        let needle = format!("\"{field}\":");
        let at = line[cursor..].find(&needle).unwrap_or_else(|| {
            panic!("`{field}` is missing or out of order in {line}");
        });
        cursor += at;
    }
    assert!(
        !line.contains("null"),
        "absent data is absent, never null: {line}"
    );
    line
}

fn json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_str(&one_ordered_object(output)).expect("one JSON object")
}

#[test]
fn a_finished_run_is_one_compact_object_in_the_contract_order() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed.")]);
    let output = batch(
        &env,
        &mock,
        &["-b", "summarise this", "--format", "json"],
        "--- a/x\n+++ b/x\n",
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let line = one_ordered_object(&output);
    assert!(
        line.starts_with(r#"{"answer":"Two files changed.","session":""#),
        "byte-exact up to the session: {line}"
    );

    let object = json(&output);
    assert_eq!(object["answer"], "Two files changed.");
    assert_eq!(object["turns"], 1);
    assert_eq!(object["stopped"], "final");
    assert_eq!(object["model"], "claude-mock");
    assert_eq!(object["tool_calls"], 0);
    assert_eq!(
        object["usage"],
        serde_json::json!({"input_tokens": 11, "output_tokens": 7, "total_tokens": 18}),
        "what the provider billed for: {line}"
    );
    assert!(
        object["duration_ms"].as_u64().is_some(),
        "the run's duration is a number of milliseconds: {line}"
    );
    assert!(
        object["session"]
            .as_str()
            .is_some_and(|session| !session.is_empty()),
        "the session GUID the run was recorded under: {line}"
    );
    assert_eq!(
        stderr(&output),
        "",
        "a script's stdout is the object and stderr stays silent"
    );
}

#[test]
fn a_run_that_called_tools_reports_how_many_and_what_they_cost() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "true", ""),
        Reply::answer("Done."),
    ]);
    let output = batch(&env, &mock, &["-b", "q", "--format", "json"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let object = json(&output);
    assert_eq!(object["turns"], 2, "one turn to call, one to answer");
    assert_eq!(object["tool_calls"], 1);
    assert_eq!(
        object["usage"],
        serde_json::json!({
            "input_tokens": 35,
            "output_tokens": 12,
            "cached_input_tokens": 4,
            "total_tokens": 47,
        }),
        "both turns' counters are summed, and one nobody reported stays absent"
    );
}

#[test]
fn verbose_stays_on_stderr_when_the_output_is_json() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "true", ""),
        Reply::answer("Done."),
    ]);
    let output = batch(&env, &mock, &["-b", "q", "--format", "json", "-v"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let object = json(&output);
    let verbose = stderr(&output);
    assert!(
        verbose.contains("sindri: bash (turn 1)"),
        "-v reports the run on stderr: {verbose}"
    );
    assert!(
        verbose.contains(
            object["session"]
                .as_str()
                .expect("the object carries its session")
        ),
        "the footer names the session the object points at: {verbose}"
    );
}

#[test]
fn exit_codes_do_not_depend_on_the_output_format() {
    // The turn cap still exits 100, and says why in the object.
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "true", ""),
        Reply::tool_call("t2", "true", ""),
    ]);
    let output = batch(
        &env,
        &mock,
        &["-b", "q", "--format", "json", "--max-turns", "1"],
        "",
    );
    assert_eq!(output.status.code(), Some(100), "the cap is exit 100");
    let object = json(&output);
    assert_eq!(object["stopped"], "max_turns", "the object says why");

    // A usage error still exits 2 with an empty stdout.
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);
    let output = batch(&env, &mock, &["-b", "--format", "json"], " \n");
    assert_eq!(output.status.code(), Some(2), "no prompt is a usage error");
    assert_eq!(stdout(&output), "", "a refusal prints no object");

    // An unknown format is a usage error, not a fallback.
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);
    let output = batch(&env, &mock, &["-b", "q", "--format", "yaml"], "");
    assert_eq!(output.status.code(), Some(2), "an unknown format is exit 2");
    assert_eq!(
        stdout(&output),
        "",
        "nothing is printed for a refused format"
    );
    assert_eq!(mock.requests().len(), 0, "nothing is asked");

    // A provider failure still exits 1 with an empty stdout.
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::error(400, "invalid_request_error", "nope")]);
    let output = batch(&env, &mock, &["-b", "q", "--format", "json"], "");
    assert_eq!(output.status.code(), Some(1), "a provider error is exit 1");
    assert_eq!(stdout(&output), "", "a failure prints no object");
}

#[test]
fn text_is_the_default_format_and_says_so_when_asked() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed.")]);
    let output = batch(&env, &mock, &["-b", "q", "--format", "text"], "");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Two files changed.\n");
}
