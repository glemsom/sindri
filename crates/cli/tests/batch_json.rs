//! The process seam for `--format json`: one compact object on one
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
const FIELD_ORDER: [&str; 9] = [
    "answer",
    "session",
    "turns",
    "stopped",
    "model",
    "duration_ms",
    "tool_calls",
    "usage",
    "enforcement",
];

fn ordered(output: &std::process::Output) -> (String, serde_json::Value) {
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
    let object = serde_json::from_str(&line).expect("the line is one JSON object");
    (line, object)
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
    let (line, object) = ordered(&output);
    assert!(
        line.starts_with(r#"{"answer":"Two files changed.","session":""#),
        "byte-exact up to the session: {line}"
    );

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
    assert_eq!(
        object["enforcement"], "bubblewrap",
        "a sandboxed run says what the reader may trust: {line}"
    );
    assert!(
        object["duration_ms"].as_u64().is_some(),
        "the run's duration is a number of milliseconds: {line}"
    );
    let session = object["session"]
        .as_str()
        .filter(|session| !session.is_empty())
        .expect("the session GUID the run was recorded under");
    let duration = object["duration_ms"]
        .as_u64()
        .expect("the run's duration in milliseconds");

    assert_eq!(
        line,
        format!(
            concat!(
                r#"{{"answer":"Two files changed.","session":"{}","turns":1,"#,
                r#""stopped":"final","model":"claude-mock","duration_ms":{},"#,
                r#""tool_calls":0,"usage":{{"input_tokens":11,"output_tokens":7,"#,
                r#""total_tokens":18}},"#,
                r#""enforcement":"bubblewrap"}}"#
            ),
            session, duration
        ),
        "stdout is byte-exact: the whole object, in order, one line"
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
    let (_, object) = ordered(&output);
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
    let (_, object) = ordered(&output);
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
    let (_, object) = ordered(&output);
    assert_eq!(object["stopped"], "max_turns", "the object says why");

    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);
    let output = batch(&env, &mock, &["-b", "--format", "json"], " \n");
    assert_eq!(output.status.code(), Some(2), "no prompt is a usage error");
    assert_eq!(stdout(&output), "", "a refusal prints no object");

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

#[test]
fn an_unsafe_run_says_so_last_in_the_object_and_once_on_stderr() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed.")]);
    let output = batch(
        &env,
        &mock,
        &["-b", "summarise this", "--format", "json", "--yolo-unsafe"],
        "",
    );

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let (line, object) = ordered(&output);
    assert_eq!(
        object["enforcement"], "unsafe",
        "the object states the policy the run really had: {line}"
    );
    assert!(
        line.ends_with(r#","enforcement":"unsafe"}"#),
        "the field appends after every existing one: {line}"
    );
    assert_eq!(
        stderr(&output),
        "sindri: --yolo-unsafe: commands run with your own privileges, outside bubblewrap\n",
        "exactly one warning line, and nothing else"
    );
}

#[test]
fn text_output_is_the_answer_and_the_warning_is_still_only_on_stderr() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed.")]);
    let output = batch(&env, &mock, &["-b", "q", "--yolo-unsafe"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Two files changed.\n",
        "stdout is the answer"
    );
    assert_eq!(
        stderr(&output).lines().count(),
        1,
        "one warning, no more: {}",
        stderr(&output)
    );
}
