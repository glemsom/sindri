#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

//! The process seam for session records (ADR-0006): the directory a run leaves
//! on disk, the id the output names, `sessions show` reading it back, and the
//! record a cancelled or killed run still leaves. Everything here is asserted
//! from outside the binary, on files and exit codes.

mod common;

use std::path::{Path, PathBuf};

use common::provider::{MockProvider, Reply};
use common::{Env, TEST_KEY, batch, signal, spawn, stderr, stdout, wait_until};

fn sessions(env: &Env) -> Vec<PathBuf> {
    let root = env.state.join("sindri/sessions");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

fn the_session(env: &Env) -> PathBuf {
    let found = sessions(env);
    assert_eq!(found.len(), 1, "one run, one record: {found:?}");
    found.into_iter().next().expect("the session directory")
}

fn everything_written(dir: &Path) -> String {
    let mut all = String::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                all.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    all
}

fn wait_for_the_sandbox_account(env: &Env) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        let seen = sessions(env).iter().any(|session| {
            std::fs::read_to_string(session.join("status.json"))
                .is_ok_and(|text| text.contains("child-pid"))
        });
        if seen {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the sandbox's child-pid document never appeared");
}

fn metadata(session: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(session.join("session.json")).expect("session.json");
    serde_json::from_str(&text).expect("session.json is one JSON object")
}

fn transcript(session: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(session.join("transcript.jsonl")).expect("transcript.jsonl");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("one entry per line"))
        .collect()
}

#[test]
fn every_run_leaves_a_record_naming_its_sandbox() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "echo twelve | wc -l", ""),
        Reply::answer("Twelve files."),
    ]);
    let output = batch(
        &env,
        &mock,
        &["-b", "how many files?", "--format", "json"],
        "",
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let printed: serde_json::Value =
        serde_json::from_str(stdout(&output).trim_end()).expect("the object");
    let id = printed["session"]
        .as_str()
        .expect("the id the output names");
    let session = the_session(&env);
    assert_eq!(
        session.file_name().and_then(|name| name.to_str()),
        Some(id),
        "the printed session is the directory the run wrote"
    );

    let meta = metadata(&session);
    assert_eq!(meta["id"], id);
    assert_eq!(meta["provider"], "anthropic");
    assert_eq!(meta["model"], "claude-sonnet-4-6");
    assert_eq!(
        meta["enforcement"], "bubblewrap",
        "the record states the sandbox the run had"
    );
    assert_eq!(meta["stopped"], "final");
    assert_eq!(meta["turns"], printed["turns"]);
    assert_eq!(meta["tool_calls"], printed["tool_calls"]);
    assert_eq!(meta["usage"], printed["usage"], "the record keeps the bill");
    assert!(meta["started_ms"].as_u64().is_some(), "{meta}");
    assert!(meta["ended_ms"].as_u64().is_some(), "{meta}");

    let entries = transcript(&session);
    let kinds: Vec<&str> = entries
        .iter()
        .map(|entry| entry["type"].as_str().expect("a typed entry"))
        .collect();
    assert_eq!(
        kinds,
        vec!["user", "assistant", "tool_call", "tool_result", "assistant"],
        "the transcript says what happened, in the order it happened: {entries:?}"
    );
    assert_eq!(entries[0]["text"], "how many files?");
    assert_eq!(entries[2]["command"], "echo twelve | wc -l");
    assert!(
        entries[3]["content"]
            .as_str()
            .is_some_and(|content| content.contains("outcome: exited")),
        "the result the model read is the result the record holds: {entries:?}"
    );

    assert!(
        session.join("tmp/home").is_dir(),
        "the session temp, with the sandbox HOME beneath it"
    );
    let status = std::fs::read_to_string(session.join("status.json")).expect("status.json");
    assert!(
        status.contains("child-pid"),
        "bubblewrap's own documents are kept: {status}"
    );
}

#[test]
fn sessions_show_prints_the_transcript_of_that_run() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "echo twelve | wc -l", ""),
        Reply::answer("Twelve files."),
    ]);
    let output = batch(
        &env,
        &mock,
        &["-b", "how many files?", "--format", "json"],
        "",
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let id = serde_json::from_str::<serde_json::Value>(stdout(&output).trim_end())
        .expect("the object")["session"]
        .as_str()
        .expect("the id")
        .to_string();

    let shown = env.cmd().args(["sessions", "show", &id]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    assert_eq!(stderr(shown), "", "a readable record is not an error");

    let text = stdout(shown);
    for expected in [
        id.as_str(),
        "sandbox: bubblewrap",
        "stopped: final",
        "--- user ---",
        "how many files?",
        "--- bash (turn 1) t1 ---",
        "$ echo twelve | wc -l",
        "--- result (turn 1) t1 ---",
        "outcome: exited",
        "Twelve files.",
    ] {
        assert!(
            text.contains(expected),
            "the transcript should read like the run; missing {expected:?} from:\n{text}"
        );
    }
}

#[test]
fn a_session_id_that_cannot_be_one_is_refused() {
    let env = Env::new();

    let missing = env
        .cmd()
        .args(["sessions", "show", "0123456789abcdef"])
        .assert();
    let missing = missing.get_output();
    assert_eq!(
        missing.status.code(),
        Some(2),
        "no such session is a usage error"
    );
    assert_eq!(stdout(missing), "", "and nothing is printed for it");
    assert!(
        !stderr(missing).is_empty(),
        "but it says which session it looked for"
    );

    // An id is never a path, so a lookup cannot leave the state directory.
    let climbing = env.cmd().args(["sessions", "show", "../../etc"]).assert();
    let climbing = climbing.get_output();
    assert_eq!(climbing.status.code(), Some(2), "a path is not an id");
    assert_eq!(stdout(climbing), "");
    assert!(sessions(&env).is_empty(), "and nothing was read or written");
}

#[test]
fn a_cancelled_run_still_leaves_a_readable_record() {
    let env = Env::new();
    let started = env.workspace.join("started");
    let script = format!("touch {}; sleep 30", started.display());
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_slow", &script, "Working.")]);

    let child = spawn(&env, &mock, &["-b", "probe the tree"]);
    wait_until(&started);
    signal(child.id(), libc::SIGINT);
    let output = child.wait_with_output().expect("sindri answers");
    assert_eq!(output.status.code(), Some(130), "a cancelled run says so");

    let session = the_session(&env);
    let meta = metadata(&session);
    assert_eq!(meta["stopped"], "cancelled", "the record says why it ended");
    assert!(meta["ended_ms"].as_u64().is_some(), "{meta}");
    assert_eq!(stderr(&output), "", "cancelling is not an error to report");

    let entries = transcript(&session);
    assert_eq!(entries[0]["type"], "user", "the prompt survived the cancel");
    assert_eq!(entries[2]["type"], "tool_call", "so did the command it ran");

    let id = meta["id"].as_str().expect("the id").to_string();
    let shown = env.cmd().args(["sessions", "show", &id]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    assert!(
        stdout(shown).contains(&script),
        "the cancelled command is on the record:\n{}",
        stdout(shown)
    );
}

#[test]
fn a_cancelled_run_says_on_stderr_where_its_record_is() {
    let env = Env::new();
    let started = env.workspace.join("started");
    let script = format!("touch {}; sleep 30", started.display());
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_slow", &script, "Working.")]);

    let child = spawn(&env, &mock, &["-b", "probe the tree", "-v"]);
    wait_until(&started);
    signal(child.id(), libc::SIGINT);
    let output = child.wait_with_output().expect("sindri answers");

    assert_eq!(output.status.code(), Some(130), "a cancelled run says so");
    assert_eq!(stdout(&output), "", "and still writes nothing to stdout");

    let malformed = stderr(&output);
    let id = metadata(&the_session(&env))["id"]
        .as_str()
        .expect("the id")
        .to_string();
    assert!(
        malformed.contains(&format!("sindri: session {id}")),
        "-v names the record it left: {malformed}"
    );
    assert!(
        malformed.contains("bash (turn 1)"),
        "and the command it was running: {malformed}"
    );
}

#[test]
fn a_killed_run_leaves_its_transcript_and_the_sandbox_account() {
    let env = Env::new();
    let started = env.workspace.join("started");
    let script = format!("touch {}; sleep 30", started.display());
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_slow", &script, "Working.")]);

    let mut child = spawn(&env, &mock, &["-b", "probe the tree"]);
    wait_until(&started);
    wait_for_the_sandbox_account(&env);
    signal(child.id(), libc::SIGKILL);
    let _ = child.wait();

    let session = the_session(&env);
    let meta = metadata(&session);
    assert_eq!(
        meta["stopped"], "running",
        "a run that never ended says so rather than guessing"
    );
    assert!(meta["ended_ms"].is_null(), "there is no end to report");

    let status = std::fs::read_to_string(session.join("status.json")).expect("status.json");
    assert!(
        status.contains("child-pid"),
        "the sandbox's own document arrived before the kill: {status}"
    );

    let id = meta["id"].as_str().expect("the id").to_string();
    let shown = env.cmd().args(["sessions", "show", &id]).assert();
    let shown = shown.get_output();
    assert_eq!(
        shown.status.code(),
        Some(0),
        "a run that never finished still reads: {}",
        stderr(shown)
    );
    assert!(stdout(shown).contains(&script), "{}", stdout(shown));
}

#[test]
fn a_credential_never_reaches_the_record() {
    let env = Env::new();
    let leaky = format!("curl -H 'x-api-key: {TEST_KEY}' example.invalid");
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", &leaky, ""),
        Reply::answer("Sent."),
    ]);
    let prompt = format!("send {TEST_KEY} somewhere");
    let output = batch(&env, &mock, &["-b", &prompt, "--format", "json", "-v"], "");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let session = the_session(&env);
    let written = everything_written(&session);
    assert!(
        !written.contains(TEST_KEY),
        "the credential must be redacted before anything is written: {written}"
    );
    assert!(
        written.contains("[redacted]"),
        "and the redaction must be visible, not the text quietly dropped: {written}"
    );

    let id = metadata(&session)["id"]
        .as_str()
        .expect("the id")
        .to_string();
    let shown = env.cmd().args(["sessions", "show", &id]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    assert!(!stdout(shown).contains(TEST_KEY), "{}", stdout(shown));
}
