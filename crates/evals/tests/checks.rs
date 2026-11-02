//! The checks that must fail loudly: an eval whose assertions cannot fail is a
//! green light that proves nothing, and these are the ones the product's
//! promises rest on — the sandbox refusing, the cap stopping, and a rule the
//! answer can only have learned from a skill it loaded.

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

#[test]
fn an_answer_that_skipped_the_skill_call_is_reported() {
    // The answer is right and the script is right; the call is missing, and
    // that is the one thing this eval exists to see.
    let root = tempfile::TempDir::new().expect("temp dir");
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("the script's directory");
    std::fs::write(
        repo.join("src/uptime.sh"),
        "#!/bin/sh\n# house: spindle-4411\nexec uptime -p\n",
    )
    .expect("the script the task asked for");
    let skipped = Run {
        exit_code: 0,
        stdout: "{\"answer\":\"Added src/uptime.sh with # house: spindle-4411 on its second line.\",\"turns\":1,\"stopped\":\"final\",\"tool_calls\":0}\n".to_string(),
        stderr: String::new(),
        requests: vec![Request::literal("POST", "/v1/messages", "{\"messages\":[]}")],
        workspace: repo,
        state: root.path().join("state"),
        live: false,
    };
    let problems = sindri_evals::checks::the_catalog_triggers(&skipped);
    assert!(
        problems.iter().any(|line| line.contains("never loaded")),
        "a skill the run did not load must be a finding: {problems:?}"
    );
}

#[test]
fn a_load_whose_rule_never_reaches_the_answer_is_reported() {
    // Everything else holds — one record, the body in the prompt, the script
    // carrying the rule — and the answer states it nowhere.
    let root = tempfile::TempDir::new().expect("temp dir");
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("the script's directory");
    std::fs::write(
        repo.join("src/uptime.sh"),
        "#!/bin/sh\n# house: spindle-4411\nexec uptime -p\n",
    )
    .expect("the script the task asked for");
    let session = root.path().join("state/sindri/sessions/one");
    std::fs::create_dir_all(&session).expect("the session directory");
    std::fs::write(
        session.join("session.json"),
        "{\"enforcement\":\"bubblewrap\"}",
    )
    .expect("the session record");
    std::fs::write(
        session.join("transcript.jsonl"),
        "{\"type\":\"tool_call\",\"turn\":1,\"call_id\":\"c1\",\"tool\":\"skill\",\"name\":\"house-style\"}\n\
         {\"type\":\"tool_result\",\"turn\":1,\"call_id\":\"c1\",\"content\":\"outcome: loaded\",\"is_error\":false,\"outcome\":\"loaded\"}\n",
    )
    .expect("the transcript");
    let quiet = Run {
        exit_code: 0,
        stdout: "{\"answer\":\"Added src/uptime.sh.\",\"turns\":2,\"stopped\":\"final\",\"tool_calls\":1}\n"
            .to_string(),
        stderr: String::new(),
        requests: vec![
            Request::literal("POST", "/v1/messages", "{\"messages\":[]}"),
            Request::literal(
                "POST",
                "/v1/messages",
                "{\"messages\":[{\"content\":\"<skill_content>\n# house: spindle-4411\n</skill_content>\"}]}",
            ),
        ],
        workspace: repo,
        state: root.path().join("state"),
        live: false,
    };
    let problems = sindri_evals::checks::the_catalog_triggers(&quiet);
    assert_eq!(
        problems,
        vec!["the answer never states the house rule \"# house: spindle-4411\""],
        "the answer is the only thing missing"
    );
}
