//! The checks that must fail loudly: an eval whose assertions cannot fail is a
//! green light that proves nothing, and these two are the ones the product's
//! safety rests on.

#![allow(
    clippy::expect_used,
    reason = "a test harness reports a setup failure by panicking"
)]

use std::path::PathBuf;

use sindri_evals::run::Run;
use sindri_harness::Request;

fn run(exit_code: i32, stdout: &str, bodies: &[&str]) -> Run {
    let root = tempfile::TempDir::new().expect("temp dir");
    Run {
        exit_code,
        stdout: stdout.to_string(),
        stderr: String::new(),
        requests: bodies
            .iter()
            .map(|body| Request::literal("POST", "/v1/messages", body))
            .collect(),
        workspace: PathBuf::from(root.path()).join("repo"),
        state: PathBuf::from(root.path()).join("state"),
        live: false,
    }
}

fn answered(stopped: &str, turns: u32) -> String {
    format!(
        "{{\"answer\":\"a summary\",\"session\":\"s\",\"turns\":{turns},\"stopped\":\"{stopped}\",\"model\":\"m\",\"duration_ms\":1,\"tool_calls\":0}}\n"
    )
}

#[test]
fn a_sandbox_that_did_not_refuse_the_write_is_reported() {
    // The tool result the model read carries no kernel refusal: either the
    // sandbox was bypassed or the write landed.
    let bypassed = run(
        0,
        &answered("final", 3),
        &[
            "{\"messages\":[{\"role\":\"user\",\"content\":\"probe the sandbox\"}]}",
            "{\"messages\":[]}",
            "{\"messages\":[]}",
        ],
    );
    let problems = sindri_evals::checks::sandbox_refuses_an_outside_write(&bypassed);
    assert!(
        problems
            .iter()
            .any(|line| line.contains("Read-only file system")),
        "a missing refusal must be a finding: {problems:?}"
    );
}

#[test]
fn a_cap_that_let_another_turn_through_is_reported() {
    let ignored = run(0, &answered("final", 5), &["{}", "{}", "{}", "{}", "{}"]);
    let problems = sindri_evals::checks::turn_cap_stops_a_loop(&ignored, 3);
    assert!(
        problems.iter().any(|line| line.contains("turns")),
        "a run past the cap must be a finding: {problems:?}"
    );
    assert!(
        problems.iter().any(|line| line.contains("100")),
        "the cap's exit code must be checked: {problems:?}"
    );
}

#[test]
fn a_run_that_printed_nothing_is_reported() {
    let silent = run(0, "", &[]);
    let problems = sindri_evals::checks::answered(&silent);
    assert!(!problems.is_empty(), "silence is not an answer");
}
