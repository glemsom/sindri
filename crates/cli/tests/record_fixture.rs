//! The record a test writes is the record a run writes: one `session.json`,
//! described by the facts a test names and by nothing else. A real run leaves
//! one, and this reads the two documents against each other — refusing a field
//! the fixture writes that no run does, and a fact a listing reads that the
//! fixture leaves out.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::provider::{MockProvider, Reply};
use common::records::{NEWEST, root};
use common::{Env, batch, stderr, stdout, the_session};
use sindri_harness::record::{Record, write};

/// The fields a record carries, as the document itself spells them.
fn fields(document: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(document).expect("session.json");
    let meta: serde_json::Value = serde_json::from_str(&text).expect("one JSON object");
    meta.as_object()
        .expect("a document")
        .keys()
        .cloned()
        .collect()
}

/// The record a run leaves, out of the one thing that writes one: a batch run in
/// a repository carrying its own rules, so the metadata a run has only sometimes
/// — its git facts and the file it was handed — are there to be read.
fn the_record_a_run_leaves() -> BTreeSet<String> {
    let env = Env::new();
    env.repository();
    std::fs::write(env.workspace.join("AGENTS.md"), "# house\n\nBe brief.\n").expect("the rules");
    let mock = MockProvider::start(vec![Reply::answer("Done.")]);
    let output = batch(&env, &mock, &["-b", "say done", "--format", "json"], "");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    fields(&the_session(&env).join("session.json"))
}

#[test]
fn the_record_a_fixture_writes_carries_the_fields_a_run_writes() {
    let run_fields = the_record_a_run_leaves();

    // The fixture's own record, saying every fact a listing can read off one.
    let env = Env::new();
    let sessions = root(&env);
    let rules = "# house\n\nBe brief.\n";
    let path = env.workspace.join("AGENTS.md");
    std::fs::write(&path, rules).expect("the rules");
    write(
        &sessions,
        &Record {
            prompt: Some("how many files?".to_string()),
            started_ms: 1_700_000_120_000,
            turns: Some(1),
            tool_calls: Some(0),
            usage: Some(serde_json::json!({ "total_tokens": 7 })),
            policy: Some("read the workspace, write nowhere".to_string()),
            answered_model: Some("claude-new".to_string()),
            workspace: "/workspace".to_string(),
            instructions: Some((path, rules.len())),
            thinking: true,
            ..Record::new(NEWEST, "claude-new", "final")
        },
    );

    let fixture_fields = fields(&sessions.join(NEWEST).join("session.json"));
    for field in &fixture_fields {
        assert!(
            run_fields.contains(field),
            "the fixture writes `{field}`, and no run writes one: {run_fields:?}"
        );
    }

    // And the other way: the row a listing prints says every fact the fixture
    // wrote, so a field the fixture dropped is a field a listing has lost.
    let listed = env.cmd().args(["sessions", "list", "--json"]).assert();
    let listed = listed.get_output();
    assert_eq!(listed.status.code(), Some(0), "stderr: {}", stderr(listed));
    let row: serde_json::Value =
        serde_json::from_str(stdout(listed).trim_end()).expect("one object per line");
    for (field, value) in [
        ("id", serde_json::json!(NEWEST)),
        ("started_ms", serde_json::json!(1_700_000_120_000_u64)),
        ("model", serde_json::json!("claude-new")),
        ("turns", serde_json::json!(1)),
        ("stopped", serde_json::json!("final")),
        ("usage", serde_json::json!({ "total_tokens": 7 })),
        ("prompt", serde_json::json!("how many files?")),
    ] {
        assert_eq!(
            row[field], value,
            "a listing reads `{field}` off a record, so the fixture writes it: {row}"
        );
    }
}
