//! A repository's `AGENTS.md` at the process seam: the text the wire carries on
//! every turn, the transcript that does not carry it, the record's start block,
//! and the two diagnostics that name the file a run stands in.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::path::{Path, PathBuf};

use common::provider::{MockProvider, Reply};
use common::{Env, stderr, stdout, system_text};
use sindri_harness::git;

/// The rule a run must be working to: distinctive enough that no other text in
/// a request could hold it.
const RULES: &str = "Always run `make audit` before you commit anything.\n";

/// A checkout with the rules at its root, and a workspace three directories
/// deep inside it — where the run stands and where the file must be found.
fn checkout(env: &Env, rules: &str) -> PathBuf {
    git::init(&env.workspace);
    std::fs::write(env.workspace.join("AGENTS.md"), rules).expect("the repository's rules");
    git::commit(&env.workspace, "the rules");
    let deep = env.workspace.join("crates/deep/inner");
    std::fs::create_dir_all(&deep).expect("a workspace three directories in");
    deep
}

/// One run, from `dir`, against the mock: two turns, so "every turn" is a claim
/// the test can make of more than one request.
fn run(env: &Env, mock: &MockProvider, dir: &Path, extra: &[&str]) -> std::process::Output {
    let mut cmd = env.cmd();
    cmd.current_dir(dir)
        .env("SINDRI_API_KEY", common::TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .env("SINDRI_BASE_URL", mock.base_url())
        .args(["-b", "say done", "--format", "json"])
        .args(extra)
        .write_stdin("")
        .assert()
        .get_output()
        .clone()
}

fn two_turns() -> MockProvider {
    MockProvider::start(vec![
        Reply::tool_call("call-1", "echo one", ""),
        Reply::answer("done"),
    ])
}

fn id_of(output: &std::process::Output) -> String {
    serde_json::from_str::<serde_json::Value>(stdout(output).trim_end())
        .expect("the object")["session"]
        .as_str()
        .expect("the id")
        .to_string()
}

#[test]
fn the_roots_rules_reach_the_model_on_every_turn() {
    let env = Env::new();
    let deep = checkout(&env, RULES);
    let mock = two_turns();

    let output = run(&env, &mock, &deep, &[]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "a call and the answer after it");
    for (index, request) in requests.iter().enumerate() {
        let system = system_text(request);
        assert!(
            system.contains("make audit"),
            "turn {} carries the repository's rules: {system}",
            index + 1
        );
    }
}

#[test]
fn the_transcript_keeps_only_the_users_words() {
    let env = Env::new();
    let deep = checkout(&env, RULES);
    let mock = two_turns();

    let output = run(&env, &mock, &deep, &[]);
    let id = id_of(&output);

    let shown = env.cmd().args(["sessions", "show", &id]).assert();
    let shown = shown.get_output();
    let text = stdout(shown);
    assert!(
        text.contains("--- user ---\nsay done"),
        "the user's prompt is the only thing in the transcript: {text}"
    );
    assert!(
        !text.contains("make audit"),
        "the rules are not a user turn: {text}"
    );
}

#[test]
fn sessions_show_names_the_file_and_its_bytes() {
    let env = Env::new();
    let deep = checkout(&env, RULES);
    let mock = MockProvider::start(vec![Reply::answer("done")]);

    let output = run(&env, &mock, &deep, &[]);
    let id = id_of(&output);
    let shown = env.cmd().args(["sessions", "show", &id]).assert();
    let text = stdout(shown.get_output());

    let line = format!(
        "instructions: {} ({} bytes)",
        env.workspace.join("AGENTS.md").display(),
        RULES.len()
    );
    assert!(
        text.contains(&line),
        "the start block names the file and what it was given:\n{text}"
    );
}

#[test]
fn verbose_and_doctor_name_the_file_in_the_same_words() {
    let env = Env::new();
    let deep = checkout(&env, RULES);
    let mock = MockProvider::start(vec![Reply::answer("done")]);

    let output = run(&env, &mock, &deep, &["-v"]);
    let verbose = stderr(&output);
    let doctor = env.cmd().arg("doctor").current_dir(&deep).assert();
    let doctor = doctor.get_output();
    let report = stdout(doctor);

    let line = format!(
        "instructions: {} ({} bytes)",
        env.workspace.join("AGENTS.md").display(),
        RULES.len()
    );
    assert!(verbose.contains(&line), "-v names it: {verbose}");
    assert!(
        report.contains(&line),
        "and doctor, in the same words: {report}"
    );
}

#[test]
fn a_repository_with_no_instructions_adds_nothing_and_one_with_instructions_appends_them() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("done")]);
    let bare_run = run(&env, &mock, &env.workspace.clone(), &[]);
    assert_eq!(
        bare_run.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&bare_run)
    );

    let deep = checkout(&env, RULES);
    let other = MockProvider::start(vec![Reply::answer("done")]);
    let ruled_run = run(&env, &other, &deep, &[]);
    assert_eq!(
        ruled_run.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&ruled_run)
    );

    let before = system_text(&mock.requests()[0]);
    let after = system_text(&other.requests()[0]);

    // What follows the preamble, written out here as the contract rather than
    // read back off the message: nothing at all where the repository has none,
    // and their one lead-in line — naming the file at the repository's root, not
    // the workspace three directories in — then the file's own bytes where it has
    // them. What the preamble's own facts say is `tests/preamble.rs`'s business,
    // and this file copies none of it.
    assert!(
        before.contains("Environment\n") && !before.contains("make audit"),
        "a workspace with no rules sends the preamble alone: {before}"
    );
    let rules = format!(
        "## Repository instructions ({})\n\n<project_instructions>\n{}\n</project_instructions>",
        env.canonical_workspace().join("AGENTS.md").display(),
        RULES.trim_end()
    );
    assert!(
        after.contains("Environment\n") && after.ends_with(&format!("\n\n{rules}")),
        "and a run with rules carries them after the preamble, unchanged: {after}"
    );
}

#[test]
fn an_unreadable_agents_md_is_a_diagnostic_and_the_run_starts() {
    let env = Env::new();
    // A directory where the file belongs: unreadable as a file, whatever uid
    // the test runs as.
    std::fs::create_dir(env.workspace.join("AGENTS.md")).expect("a directory in its place");
    let mock = MockProvider::start(vec![Reply::answer("done")]);

    let output = run(&env, &mock, &env.workspace.clone(), &[]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "an unreadable rules file never stops a run: {}",
        stderr(&output)
    );
    let said = stderr(&output);
    assert!(
        said.contains("instructions:") && said.contains("AGENTS.md") && said.contains("cannot"),
        "the diagnostic names the file and the reason: {said}"
    );
    assert_eq!(
        stdout(&output).matches("done").count(),
        1,
        "and the answer is still the answer"
    );
}
