#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

//! The record's boundary as a user finds it after the fact: the policy the model
//! was handed, the tree the run stood in, and the dials it obeyed — asserted
//! from outside the binary, on the `session.json` a run leaves, and against what
//! `sindri doctor` prints for the same overrides.

mod common;

use std::path::Path;

use common::provider::{MockProvider, Reply};
use common::{Env, batch, stderr, stdout, the_session};

fn metadata(session: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(session.join("session.json")).expect("session.json");
    serde_json::from_str(&text).expect("session.json is one JSON object")
}

/// A repository with one commit, so a record has a branch and a commit to name.
fn repository(dir: &Path) -> (String, String) {
    let author = [
        "-c",
        "user.name=sindri",
        "-c",
        "user.email=sindri@example.invalid",
    ];
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(author)
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "--initial-branch=main"]);
    std::fs::write(dir.join("readme"), "one line\n").expect("a file to commit");
    git(&["add", "readme"]);
    git(&["commit", "-q", "-m", "the first commit"]);
    (
        git(&["rev-parse", "--abbrev-ref", "HEAD"]),
        git(&["rev-parse", "HEAD"]),
    )
}

/// The record a post-mortem opens with: the boundary, the tree and the dials —
/// the same policy text `sindri doctor` prints for the same overrides, not a
/// paraphrase of it.
#[test]
fn a_batch_run_records_the_boundary_the_tree_and_the_dials() {
    let env = Env::new();
    let (branch, commit) = repository(&env.workspace);
    // An untracked file, so `dirty` is a status that was read rather than a
    // default that happens to look right.
    std::fs::write(env.workspace.join("uncommitted"), "wip\n").expect("an untracked file");
    let mock = MockProvider::start(vec![Reply::answer("Done.")]);
    let output = batch(
        &env,
        &mock,
        &[
            "-b",
            "q",
            "--format",
            "json",
            "--max-turns",
            "7",
            "--thinking",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let session = the_session(&env);
    let meta = metadata(&session);
    let id = meta["id"].as_str().expect("the id");

    let doctor = env
        .cmd()
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .clone();
    let doctor = String::from_utf8_lossy(&doctor.stdout).to_string();
    // The one part of the description a run cannot share with the probe: the
    // session temp belongs to the session that derived it.
    let diagnosed = doctor.replace("sessions/<session>/tmp", &format!("sessions/{id}/tmp"));
    assert_eq!(
        meta["policy"].as_str(),
        Some(diagnosed.as_str()),
        "the record holds the description doctor prints, verbatim"
    );

    assert_eq!(
        meta["workspace"].as_str(),
        env.canonical_workspace().to_str(),
        "and the tree it ran against"
    );
    assert_eq!(meta["git"]["branch"], branch.as_str());
    assert_eq!(meta["git"]["commit"], commit.as_str());
    assert_eq!(meta["git"]["dirty"], true, "{meta}");
    assert_eq!(meta["max_turns"], 7);
    assert_eq!(meta["thinking"], true);
}

#[test]
fn a_run_outside_a_repository_records_no_git_block() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Done.")]);
    let output = batch(&env, &mock, &["-b", "q", "--format", "json"], "");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let meta = metadata(&the_session(&env));
    assert!(
        meta.as_object()
            .is_some_and(|meta| !meta.contains_key("git")),
        "a directory that is not a repository records no git block at all: {meta}"
    );
}

/// A record written before the boundary was recorded: none of the fields exist,
/// and it still reads and renders — the rule the README states.
#[test]
fn a_record_from_before_these_fields_still_reads_and_renders() {
    let env = Env::new();
    let id = "abcdef0123456789";
    let session = env.state.join("sindri/sessions").join(id);
    std::fs::create_dir_all(&session).expect("a session directory");
    std::fs::write(
        session.join("session.json"),
        r#"{"id":"abcdef0123456789","version":"sindri 0.1.0","provider":"anthropic",
            "model":"claude-mock","enforcement":"bubblewrap","pid":4711,
            "started_ms":1700000000000,"stopped":"final"}"#,
    )
    .expect("an old session.json");
    std::fs::write(
        session.join("transcript.jsonl"),
        "{\"type\":\"user\",\"text\":\"how many files?\"}\n",
    )
    .expect("an old transcript");

    let shown = env.cmd().args(["sessions", "show", id]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    let text = stdout(shown);
    assert!(
        text.contains("how many files?"),
        "an old record still renders: {text}"
    );
    assert!(
        !text.contains("workspace:"),
        "and claims no tree it never recorded: {text}"
    );
}
