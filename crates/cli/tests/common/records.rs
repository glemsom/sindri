//! The records a listing test writes: the ids a row is looked for by, and the
//! three a frame could have left. The shape of `session.json` is the harness's —
//! one fixture for every test that needs a record on disk — so what is stated
//! here is only what a test's own records say about themselves.
//!
//! Every fixture here is read-only for the command under test — nothing writes
//! a record back.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

use std::path::{Path, PathBuf};

use serde_json::json;
use sindri_harness::record::{Record, write};

use super::Env;

pub const OLDEST: &str = "0f1c8f2e-1111-4a2b-8c3d-000000000001";
pub const MIDDLE: &str = "0f1c8f2e-2222-4a2b-8c3d-000000000002";
pub const NEWEST: &str = "0f1c8f2e-3333-4a2b-8c3d-000000000003";

/// The state directory a listing reads.
pub fn root(env: &Env) -> PathBuf {
    env.state.join("sindri/sessions")
}

/// Three records a frame could have left: one that finished, one the turn cap
/// stopped, and one that never finished at all.
pub fn three(sessions: &Path) {
    write(
        sessions,
        &Record {
            started_ms: 1_700_000_000_000,
            turns: Some(3),
            prompt: Some("how many files?".to_string()),
            usage: Some(json!({
                "input_tokens": 1_200,
                "output_tokens": 90,
                "total_tokens": 1_290
            })),
            ..Record::new(OLDEST, "claude-old", "final")
        },
    );
    write(
        sessions,
        &Record {
            started_ms: 1_700_000_060_000,
            turns: Some(7),
            prompt: Some("rewrite the parser".to_string()),
            usage: Some(json!({ "total_tokens": 47 })),
            ..Record::new(MIDDLE, "claude-middle", "max_turns")
        },
    );
    write(
        sessions,
        &Record {
            started_ms: 1_700_000_120_000,
            prompt: Some("why is the gate red?".to_string()),
            ..Record::new(NEWEST, "claude-new", "running")
        },
    );
}

pub const DETAILED: &str = "0f1c8f2e-4444-4a2b-8c3d-000000000004";

/// A record whose single turn reported every counter the provider has.
pub const ALL_FIVE: &str = "0f1c8f2e-5555-4a2b-8c3d-000000000005";

/// A record whose single turn reported no counter at all.
pub const NO_COUNTERS: &str = "0f1c8f2e-6666-4a2b-8c3d-000000000006";

/// Where the one truncated result's untruncated text went, as the model was
/// told: a path the render must hand back unchanged.
pub const SPILL: &str = "tmp/spill/t1.txt";

/// One record holding everything the last few tickets put in it: the run's
/// boundary and dials, what each turn cost, what the call did, and bubblewrap's
/// own documents. Written as a record rather than by a run, so `show` is proven
/// against the files a run leaves and not against the writer that made them.
pub fn detailed(sessions: &Path) {
    let dir = write(
        sessions,
        &Record {
            started_ms: 1_700_000_180_000,
            turns: Some(2),
            tool_calls: Some(1),
            prompt: Some("how many files?".to_string()),
            usage: Some(json!({ "input_tokens": 150, "output_tokens": 42, "total_tokens": 192 })),
            answered_model: Some("claude-detailed".to_string()),
            workspace: "/tmp/workspace".to_string(),
            policy: Some("read the workspace, write nowhere".to_string()),
            thinking: true,
            entries: vec![
                json!({
                    "type": "assistant",
                    "turn": 1,
                    "text": "Let me look.",
                    "reasoning": "counting the files",
                    "usage": { "input_tokens": 120, "output_tokens": 33, "total_tokens": 153 },
                    "duration_ms": 2100,
                }),
                json!({
                    "type": "tool_call",
                    "turn": 1,
                    "call_id": "t1",
                    "tool": "bash",
                    "command": "ls | wc -l",
                }),
                json!({
                    "type": "tool_result",
                    "turn": 1,
                    "call_id": "t1",
                    "content": "outcome: exited\nexit_code: 0\nstdout:\n12\n",
                    "is_error": false,
                    "outcome": "exited",
                    "exit_code": 0,
                    "duration_ms": 37,
                    "truncated": true,
                    "spill": [SPILL],
                }),
                json!({ "type": "assistant", "turn": 2, "text": "Twelve files.", "duration_ms": 900 }),
            ],
            ..Record::new(DETAILED, "claude-detailed", "final")
        },
    );

    // A kill cuts the line it was writing: the count of lines lost is a fact the
    // render states, so the fixture carries one.
    let mut transcript =
        std::fs::read_to_string(dir.join("transcript.jsonl")).expect("a transcript");
    transcript.push_str(r#"{"type":"assistant","turn":3,"te"#);
    std::fs::write(dir.join("transcript.jsonl"), transcript).expect("the cut transcript");

    let status = [
        json!({ "call": 1, "document": { "child-pid": 4242, "unshare-net": true } }),
        json!({ "call": 1, "line": "bwrap: the sandbox started" }),
    ];
    let mut documents = String::new();
    for line in status {
        documents.push_str(&line.to_string());
        documents.push('\n');
    }
    std::fs::write(dir.join("status.json"), documents).expect("status.json");
}

/// One record whose single turn reported exactly the counters given, and the run
/// totals the same: a reader that names them is proven against a turn that
/// holds them rather than against a fixture's luck.
pub fn with_counters(sessions: &Path, id: &'static str, usage: serde_json::Value) {
    write(
        sessions,
        &Record {
            started_ms: 1_700_000_240_000,
            turns: Some(1),
            prompt: Some("how many files?".to_string()),
            entries: vec![json!({
                "type": "assistant",
                "turn": 1,
                "text": "Twelve files.",
                "usage": usage,
                "duration_ms": 1200,
            })],
            usage: Some(usage),
            ..Record::new(id, "claude-counters", "final")
        },
    );
}
