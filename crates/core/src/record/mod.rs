//! The record every run leaves: `session.json` for metadata,
//! `transcript.jsonl` for what happened in order, and `status.json` for
//! bubblewrap's own documents as they arrive. Written as the run goes, so a
//! cancelled or crashed run still leaves readable evidence — nothing is buffered
//! until the end, because the end is exactly what a crash takes away.
//!
//! Writing is best effort: a full disk must not turn an answer into a failure.
//! `start` and `finish` do report trouble, because those are the two moments a
//! front end can still say something.

mod entry;
mod export;
mod redact;
mod stored;
mod summary;

pub use entry::{Entry, Meta};
pub use redact::Redaction;
pub use stored::{ShowError, Stored};
pub use summary::{Summary, listing, summaries};

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::agent::Report;
use crate::exec::StatusSink;
use crate::paths::SessionDir;
use crate::provider::{Call, ToolOutcome, Usage};
use crate::sandbox::Enforcement;
use crate::workspace::Git;
use entry::Entry as Line;

/// What a turn cost, when the loop measured it: the turn's own counters and the
/// span it took. Default is "nobody measured it", which is not the same as
/// free, so an entry never claims a zero it did not see.
#[derive(Debug, Clone, Default)]
pub struct Cost {
    /// The counters the provider reported for the turn; empty when it reported
    /// none, which the record writes as absent.
    pub usage: Usage,
    pub duration: Option<Duration>,
}

impl Cost {
    pub fn measured(usage: Usage, duration: Duration) -> Self {
        Cost {
            usage,
            duration: Some(duration),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Start {
    pub provider: String,
    pub model: String,
    pub enforcement: Enforcement,
    /// The description the model is handed, verbatim: the record keeps the one
    /// text rather than a structured copy of it. Absent only for a caller with
    /// no policy to hand over, which no run is.
    pub policy: Option<String>,
    /// The tree the run stands in, and its git facts where it is a repository.
    pub workspace: String,
    pub git: Option<Git>,
    /// The dials the run obeys.
    pub max_turns: u32,
    pub thinking: bool,
}

impl Start {
    /// A record started with nothing but the run's identity, for a caller that
    /// is not a run: the fields only a live session can probe stay empty.
    pub fn of(
        provider: impl Into<String>,
        model: impl Into<String>,
        enforcement: Enforcement,
    ) -> Start {
        Start {
            provider: provider.into(),
            model: model.into(),
            enforcement,
            policy: None,
            workspace: String::new(),
            git: None,
            max_turns: 0,
            thinking: false,
        }
    }
}

pub struct Recorder {
    root: PathBuf,
    redaction: Redaction,
    inner: Mutex<Inner>,
}

struct Inner {
    meta: Meta,
    transcript: File,
    status: File,
}

impl Recorder {
    pub fn start(session: &SessionDir, start: Start, redaction: Redaction) -> io::Result<Self> {
        let root = session.root().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let meta = Meta {
            id: session.id().to_string(),
            version: sindri_types::VERSION_LONG.to_string(),
            provider: start.provider,
            model: start.model,
            enforcement: start.enforcement.as_str().to_string(),
            pid: std::process::id(),
            started_ms: now_ms(),
            ended_ms: None,
            duration_ms: None,
            answered_model: None,
            turns: None,
            tool_calls: None,
            stopped: "running".to_string(),
            usage: None,
            error: None,
            policy: start.policy,
            workspace: start.workspace,
            git: start.git,
            max_turns: start.max_turns,
            thinking: start.thinking,
        };
        write_meta(&root, &meta)?;
        let transcript = open(root.join("transcript.jsonl"))?;
        let status = open(root.join("status.json"))?;
        let recorder = Recorder {
            root,
            redaction,
            inner: Mutex::new(Inner {
                meta,
                transcript,
                status,
            }),
        };
        Ok(recorder)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn user(&self, text: &str) {
        self.note(Line::User {
            text: self.redaction.apply(text),
        });
    }

    /// One turn: what it said and what it cost. The cost is measured by the
    /// caller, because only the loop holds a clock; an absent one means nobody
    /// measured it, which is not the same as free.
    pub fn assistant(&self, turn: u32, text: &str, reasoning: Option<&str>, cost: Cost) {
        self.note(Line::Assistant {
            turn,
            text: self.redaction.apply(text),
            reasoning: reasoning.map(|text| self.redaction.apply(text)),
            usage: (!cost.usage.is_empty()).then_some(cost.usage),
            duration_ms: cost.duration.map(millis),
        });
    }

    /// One call, written before it runs, so a command killed mid-flight is still
    /// on the record.
    pub fn tool_call(&self, turn: u32, call: &Call) {
        let (command, problem) = match call.command() {
            Ok(command) => (Some(self.redaction.apply(&command)), None),
            Err(reason) => (None, Some(self.redaction.apply(&reason))),
        };
        self.note(Line::ToolCall {
            turn,
            call_id: call.id.clone(),
            tool: call.name.clone(),
            command,
            problem,
        });
    }

    pub fn tool_result(&self, turn: u32, outcome: &ToolOutcome) {
        let facts = &outcome.facts;
        self.note(Line::ToolResult {
            turn,
            call_id: outcome.call_id.clone(),
            content: self.redaction.apply(&outcome.content),
            is_error: outcome.is_error,
            outcome: facts.outcome.clone(),
            exit_code: facts.exit_code,
            duration_ms: facts.duration_ms,
            truncated: facts.truncated,
            // Redacted like the content it is also written in, so the two
            // spellings of one path cannot disagree either.
            spill: facts
                .spill
                .iter()
                .map(|path| self.redaction.apply(path))
                .collect(),
        });
    }

    /// The boundary moved between prompts: the metadata settles on the new
    /// description and the transcript says when it moved, so a run whose
    /// writable set changed does not claim its first policy for all of it.
    /// Text identical to the last one recorded is not a change.
    pub fn rebase(&self, description: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.meta.policy.as_deref() == Some(description) {
            return;
        }
        inner.meta.policy = Some(description.to_string());
        let _ = write_meta(&self.root, &inner.meta);
        let line = Line::Policy {
            description: self.redaction.apply(description),
        };
        let _ = writeln!(inner.transcript, "{}", json(&line));
    }

    pub fn finish(&self, report: &Report) -> io::Result<()> {
        self.settle(|meta| {
            meta.stopped = report.stopped.as_str().to_string();
            meta.answered_model.clone_from(&report.model);
            meta.turns = Some(report.turns);
            meta.tool_calls = Some(report.tool_calls);
            meta.duration_ms = Some(millis(report.duration));
            meta.usage = (!report.usage.is_empty()).then(|| report.usage.clone());
        })
    }

    pub fn failed(&self, message: &str) -> io::Result<()> {
        let message = self.redaction.apply(message);
        self.settle(move |meta| {
            meta.stopped = "error".to_string();
            meta.error = Some(message);
        })
    }

    /// Rewrites the metadata once the run has an end to report, atomically: a
    /// reader never sees half a document.
    fn settle(&self, change: impl FnOnce(&mut Meta)) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("the record was poisoned"))?;
        change(&mut inner.meta);
        inner.meta.ended_ms = Some(now_ms());
        write_meta(&self.root, &inner.meta)
    }

    /// Appends one line. A write that fails is not worth failing a run over, and
    /// a short record is still evidence of everything up to that point.
    fn note(&self, line: Line) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let _ = writeln!(inner.transcript, "{}", json(&line));
    }
}

impl StatusSink for Recorder {
    /// One of bubblewrap's status documents, verbatim where it is not JSON:
    /// what a post-mortem needs is what bwrap actually said.
    fn document(&self, call: u64, document: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let line = match serde_json::from_str::<serde_json::Value>(document) {
            Ok(parsed) => serde_json::json!({ "call": call, "document": parsed }),
            Err(_) => serde_json::json!({ "call": call, "line": document }),
        };
        let _ = writeln!(inner.status, "{}", json(&line));
    }
}

/// One value as JSON. Every value written here is a struct or an object of
/// strings and numbers, which `serde_json` cannot refuse.
fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn open(path: PathBuf) -> io::Result<File> {
    File::options().create(true).append(true).open(path)
}

/// Writes the metadata beside itself, then renames it into place: a reader sees
/// the old document or the new one, never a half-written one.
fn write_meta(root: &Path, meta: &Meta) -> io::Result<()> {
    let staging = root.join("session.json.tmp");
    std::fs::write(&staging, format!("{}\n", json(meta)))?;
    std::fs::rename(&staging, root.join("session.json"))
}

/// A span in whole milliseconds, the one unit the record speaks.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
