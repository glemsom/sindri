//! The record a test writes: the `session.json` a run leaves, and the
//! transcript beside it. One fixture, because a second copy of the record's
//! shape is a file no run writes — a field renamed in the record would leave it
//! describing a document nothing reads, and the tests reading it would go on
//! passing. What a record says about itself — its id, its model, its prompt, how
//! it stopped — is the test's to name; the shape is this module's, and `cli`'s
//! `record_fixture` test holds it to the file a real run leaves.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// One record a test writes, held as the facts it says rather than as the
/// document it makes of them: a test states the record its case turns on, and
/// every other field of `session.json` is this fixture's.
#[derive(Debug)]
pub struct Record {
    /// The session's id, which is also the directory it is written in.
    pub id: String,
    pub model: String,
    /// The first thing the human asked, written as the transcript's first line.
    /// Absent for a record whose transcript opens with the calls it made.
    pub prompt: Option<String>,
    /// Why the run ended: `final`, `max_turns`, `cancelled`, `error` — or
    /// `running`, a run that never ended, which is a record with no end to state.
    pub stopped: String,
    /// The moment it started: two records are ordered by this one.
    pub started_ms: u64,
    /// The turns it took, where a test wants a row to count them.
    pub turns: Option<u32>,
    /// The commands it ran, counted as the record counts them.
    pub tool_calls: Option<u32>,
    /// What it spent, as its own metadata states it.
    pub usage: Option<Value>,
    /// The model that answered, where a test wants the record to say which.
    pub answered_model: Option<String>,
    /// The tree the run stood in; empty is a directory no run named.
    pub workspace: String,
    /// The repository's own rules the run was handed: the file, and the bytes of
    /// it the model read.
    pub instructions: Option<(PathBuf, usize)>,
    /// The boundary as the model was handed it.
    pub policy: Option<String>,
    pub max_turns: u32,
    pub thinking: bool,
    /// Every further transcript line, in order, after the prompt.
    pub entries: Vec<Value>,
}

impl Record {
    /// A record of a run that answered, with nothing else said about it yet: the
    /// id, the model and how it stopped are the three facts no record is without.
    pub fn new(id: &str, model: &str, stopped: &str) -> Self {
        Record {
            id: id.to_string(),
            model: model.to_string(),
            prompt: None,
            stopped: stopped.to_string(),
            started_ms: 1_700_000_000_000,
            turns: None,
            tool_calls: None,
            usage: None,
            answered_model: None,
            workspace: String::new(),
            instructions: None,
            policy: None,
            max_turns: 250,
            thinking: false,
            entries: Vec::new(),
        }
    }
}

/// The record, written into `sessions` as a run leaves it: the directory, its
/// metadata, and a transcript made of the prompt and the lines the test wrote.
/// The directory comes back, so a fixture that leaves more than a run does —
/// bubblewrap's documents, a line a crash cut — writes those beside it.
pub fn write(sessions: &Path, record: &Record) -> PathBuf {
    let dir = sessions.join(&record.id);
    std::fs::create_dir_all(&dir).expect("a session directory");
    let meta = format!("{}\n", meta(record));
    std::fs::write(dir.join("session.json"), meta).expect("the metadata");

    let mut transcript = String::new();
    if let Some(prompt) = &record.prompt {
        transcript.push_str(&json!({ "type": "user", "text": prompt }).to_string());
        transcript.push('\n');
    }
    for entry in &record.entries {
        transcript.push_str(&entry.to_string());
        transcript.push('\n');
    }
    std::fs::write(dir.join("transcript.jsonl"), transcript).expect("the transcript");
    dir
}

/// The metadata a run writes for this record: the record's own fields, and a
/// field with nothing to say left out rather than sent as `null` — the rule
/// `session.json` already keeps.
fn meta(record: &Record) -> Value {
    let mut meta = json!({
        "id": record.id,
        "version": env!("CARGO_PKG_VERSION"),
        "provider": "anthropic",
        "model": record.model,
        "enforcement": "bubblewrap",
        "pid": std::process::id(),
        "started_ms": record.started_ms,
        "stopped": record.stopped,
        "workspace": record.workspace,
        "max_turns": record.max_turns,
        "thinking": record.thinking,
    });
    // A run that ended states when and how long it took; one still saying
    // `running` is a record a crash left, and has neither.
    if record.stopped != "running" {
        meta["ended_ms"] = json!(record.started_ms + 1_000);
        meta["duration_ms"] = json!(1_000);
    }
    if let Some(answered) = &record.answered_model {
        meta["answered_model"] = json!(answered);
    }
    if let Some(usage) = &record.usage {
        meta["usage"] = usage.clone();
    }
    if let Some(turns) = record.turns {
        meta["turns"] = json!(turns);
    }
    if let Some(tool_calls) = record.tool_calls {
        meta["tool_calls"] = json!(tool_calls);
    }
    if let Some(policy) = &record.policy {
        meta["policy"] = json!(policy);
    }
    if let Some((path, bytes)) = &record.instructions {
        meta["instructions"] = json!({ "path": path.display().to_string(), "bytes": bytes });
    }
    meta
}
