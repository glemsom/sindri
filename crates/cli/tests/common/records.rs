//! Records a listing test writes itself: the files a run leaves, so `sessions
//! list` is proven against a state directory without starting a run.
//!
//! Every fixture here is read-only for the command under test — nothing writes
//! a record back.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

use std::path::{Path, PathBuf};

use super::Env;

pub const OLDEST: &str = "0f1c8f2e-1111-4a2b-8c3d-000000000001";
pub const MIDDLE: &str = "0f1c8f2e-2222-4a2b-8c3d-000000000002";
pub const NEWEST: &str = "0f1c8f2e-3333-4a2b-8c3d-000000000003";

/// One record as a run leaves it: the metadata, and a transcript whose first
/// user line is the thing the run was asked.
pub struct Record {
    pub id: &'static str,
    pub started_ms: u64,
    pub model: &'static str,
    pub turns: Option<u32>,
    pub stopped: &'static str,
    pub usage: Option<serde_json::Value>,
    pub prompt: Option<&'static str>,
}

/// The state directory a listing reads.
pub fn root(env: &Env) -> PathBuf {
    env.state.join("sindri/sessions")
}

pub fn write(sessions: &Path, record: &Record) {
    let dir = sessions.join(record.id);
    std::fs::create_dir_all(&dir).expect("a session directory");
    let mut meta = serde_json::json!({
        "id": record.id,
        "version": "0.1.0",
        "provider": "anthropic",
        "model": record.model,
        "enforcement": "bubblewrap",
        "pid": 4242,
        "started_ms": record.started_ms,
        "stopped": record.stopped,
        "max_turns": 250,
    });
    if let Some(turns) = record.turns {
        meta["turns"] = turns.into();
    }
    if let Some(usage) = &record.usage {
        meta["usage"] = usage.clone();
    }
    std::fs::write(dir.join("session.json"), format!("{meta}\n")).expect("session.json");
    let mut transcript = String::new();
    if let Some(prompt) = record.prompt {
        transcript.push_str(&serde_json::json!({"type":"user","text":prompt}).to_string());
        transcript.push('\n');
    }
    std::fs::write(dir.join("transcript.jsonl"), transcript).expect("transcript.jsonl");
}

/// Three records a frame could have left: one that finished, one the turn cap
/// stopped, and one that never finished at all.
pub fn three(sessions: &Path) {
    write(
        sessions,
        &Record {
            id: OLDEST,
            started_ms: 1_700_000_000_000,
            model: "claude-old",
            turns: Some(3),
            stopped: "final",
            usage: Some(serde_json::json!({
                "input_tokens": 1_200,
                "output_tokens": 90,
                "total_tokens": 1_290
            })),
            prompt: Some("how many files?"),
        },
    );
    write(
        sessions,
        &Record {
            id: MIDDLE,
            started_ms: 1_700_000_060_000,
            model: "claude-middle",
            turns: Some(7),
            stopped: "max_turns",
            usage: Some(serde_json::json!({ "total_tokens": 47 })),
            prompt: Some("rewrite the parser"),
        },
    );
    write(
        sessions,
        &Record {
            id: NEWEST,
            started_ms: 1_700_000_120_000,
            model: "claude-new",
            turns: None,
            stopped: "running",
            usage: None,
            prompt: Some("why is the gate red?"),
        },
    );
}
